//! Vendor-uninstall state machine and outcome classification.
//!
//! Completion is decided from post-conditions, never from "the process
//! exited" alone. Only `Succeeded`, `SucceededRebootRequired` and
//! `RebootInitiated` start a leftover scan automatically; every other outcome
//! requires an explicit, acknowledged, read-only scan. Sweep's own cleanup is
//! disabled in Stage A regardless of outcome.

use crate::api::{KeyState, RegRead};
use crate::model::RegKeyRef;
use crate::msi::{self, MsiProductState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason")]
pub enum UnknownReason {
    /// Exit 0 but the Uninstall entry is still present.
    EntryStillPresent,
    /// msiexec 1605: Windows Installer does not know the product.
    NotInstalled,
    /// msiexec 0 but MsiQueryProductState still reports it installed.
    ProductStillInstalled,
    /// Exit code could not be obtained (e.g. process handle lost).
    ExitCodeUnavailable,
    /// The Uninstall entry could not be read (e.g. access denied), so its
    /// removal cannot be confirmed.
    EntryUnreadable,
    /// Some uninstaller processes were not tracked to completion (could not
    /// be opened, or the user stopped waiting), so work may still be running.
    TrackingIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome")]
pub enum Outcome {
    Succeeded,
    SucceededRebootRequired,
    RebootInitiated,
    Cancelled,
    ElevationCancelled,
    Failed { code: u32, retryable: bool },
    LaunchFailed { code: u32 },
    Unknown(UnknownReason),
}

impl Outcome {
    /// Whether a read-only leftover scan may start without extra confirmation.
    pub fn auto_scan_allowed(&self) -> bool {
        matches!(self, Outcome::Succeeded | Outcome::SucceededRebootRequired | Outcome::RebootInitiated)
    }

    /// Whether Sweep's own cleanup could ever follow this outcome (Stage B).
    /// Always combined with [`crate::CLEANUP_ENABLED`], which is false in Stage A.
    pub fn cleanup_eligible(&self) -> bool {
        crate::CLEANUP_ENABLED && self.auto_scan_allowed()
    }
}

/// State of the program's Uninstall entry after the uninstaller ran.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryState {
    #[default]
    Absent,
    Present,
    Unreadable(String),
}

/// Combine the state of every registry location the program was listed in.
/// Any present key means present; otherwise any unreadable key means
/// unreadable; only all-absent counts as removed.
pub fn entry_state(reg: &dyn RegRead, keys: &[RegKeyRef]) -> EntryState {
    let mut unreadable = None;
    for k in keys {
        match reg.key_state(k.root, k.view, &k.path) {
            KeyState::Present => return EntryState::Present,
            KeyState::Unreadable(e) => unreadable = Some(format!("{k}: {e}")),
            KeyState::Absent => {}
        }
    }
    unreadable.map(EntryState::Unreadable).unwrap_or(EntryState::Absent)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Postconditions {
    /// The program's Uninstall registry entry (all recorded views).
    pub entry: EntryState,
    /// For MSI: product state after the run.
    pub msi_state: Option<MsiProductState>,
    /// New PendingFileRenameOperations entries pointing into the install dir.
    pub new_pending_renames: bool,
    /// Some uninstaller processes were not tracked until they exited.
    pub tracking_incomplete: bool,
}

/// A success-looking result is only confirmed when every tracked process
/// was seen to exit; otherwise the uninstaller may still be working.
fn confirm(success: Outcome, post: &Postconditions) -> Outcome {
    if post.tracking_incomplete {
        Outcome::Unknown(UnknownReason::TrackingIncomplete)
    } else {
        success
    }
}

pub fn classify_msi(exit: Option<u32>, post: &Postconditions) -> Outcome {
    let Some(code) = exit else {
        return Outcome::Unknown(UnknownReason::ExitCodeUnavailable);
    };
    match code {
        msi::ERROR_SUCCESS => match post.msi_state {
            Some(MsiProductState::NotInstalled) => confirm(Outcome::Succeeded, post),
            _ => Outcome::Unknown(UnknownReason::ProductStillInstalled),
        },
        msi::ERROR_INSTALL_USEREXIT => Outcome::Cancelled,
        msi::ERROR_UNKNOWN_PRODUCT => Outcome::Unknown(UnknownReason::NotInstalled),
        msi::ERROR_INSTALL_ALREADY_RUNNING => Outcome::Failed { code, retryable: true },
        msi::ERROR_SUCCESS_REBOOT_INITIATED => Outcome::RebootInitiated,
        msi::ERROR_SUCCESS_REBOOT_REQUIRED => confirm(Outcome::SucceededRebootRequired, post),
        other => Outcome::Failed { code: other, retryable: false },
    }
}

pub fn classify_exe(exit: Option<u32>, post: &Postconditions) -> Outcome {
    let Some(code) = exit else {
        return Outcome::Unknown(UnknownReason::ExitCodeUnavailable);
    };
    if code != 0 {
        return Outcome::Failed { code, retryable: false };
    }
    match &post.entry {
        EntryState::Present => return Outcome::Unknown(UnknownReason::EntryStillPresent),
        EntryState::Unreadable(_) => return Outcome::Unknown(UnknownReason::EntryUnreadable),
        EntryState::Absent => {}
    }
    let success = if post.new_pending_renames { Outcome::SucceededRebootRequired } else { Outcome::Succeeded };
    confirm(success, post)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state")]
pub enum UninstallState {
    Ready,
    Launching,
    Running { pids: Vec<u32>, tracking_incomplete: bool },
    Verifying { tracking_incomplete: bool },
    Finished { outcome: Outcome, tracking_incomplete: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Launch,
    /// ShellExecuteEx returned a process.
    Started {
        pid: u32,
    },
    /// ShellExecuteEx failed; 1223 = ERROR_CANCELLED (UAC declined).
    LaunchError {
        code: u32,
    },
    /// Descendant tracking update.
    Tracked {
        pids: Vec<u32>,
        incomplete: bool,
    },
    /// All tracked processes exited.
    Exited,
    Verified(Outcome),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid transition from {from:?} on {event:?}")]
pub struct InvalidTransition {
    pub from: UninstallState,
    pub event: Event,
}

pub const ERROR_CANCELLED: u32 = 1223;

impl UninstallState {
    pub fn next(self, event: Event) -> Result<UninstallState, InvalidTransition> {
        use UninstallState::*;
        match (&self, &event) {
            (Ready, Event::Launch) => Ok(Launching),
            (Launching, Event::Started { pid }) => Ok(Running { pids: vec![*pid], tracking_incomplete: false }),
            (Launching, Event::LaunchError { code }) => {
                let outcome = if *code == ERROR_CANCELLED {
                    Outcome::ElevationCancelled
                } else {
                    Outcome::LaunchFailed { code: *code }
                };
                Ok(Finished { outcome, tracking_incomplete: false })
            }
            (Running { tracking_incomplete, .. }, Event::Tracked { pids, incomplete }) => {
                Ok(Running { pids: pids.clone(), tracking_incomplete: *tracking_incomplete || *incomplete })
            }
            (Running { tracking_incomplete, .. }, Event::Exited) => {
                Ok(Verifying { tracking_incomplete: *tracking_incomplete })
            }
            (Verifying { tracking_incomplete }, Event::Verified(o)) => {
                Ok(Finished { outcome: o.clone(), tracking_incomplete: *tracking_incomplete })
            }
            _ => Err(InvalidTransition { from: self, event }),
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, UninstallState::Finished { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post() -> Postconditions {
        Postconditions::default()
    }

    #[test]
    fn msi_exit_codes() {
        let gone = Postconditions { msi_state: Some(MsiProductState::NotInstalled), ..post() };
        let still = Postconditions { msi_state: Some(MsiProductState::Installed), ..post() };
        assert_eq!(classify_msi(Some(0), &gone), Outcome::Succeeded);
        assert_eq!(classify_msi(Some(0), &still), Outcome::Unknown(UnknownReason::ProductStillInstalled));
        assert_eq!(classify_msi(Some(1602), &still), Outcome::Cancelled);
        assert_eq!(classify_msi(Some(1605), &gone), Outcome::Unknown(UnknownReason::NotInstalled));
        assert_eq!(classify_msi(Some(1618), &still), Outcome::Failed { code: 1618, retryable: true });
        assert_eq!(classify_msi(Some(1641), &gone), Outcome::RebootInitiated);
        assert_eq!(classify_msi(Some(3010), &gone), Outcome::SucceededRebootRequired);
        assert_eq!(classify_msi(Some(1603), &still), Outcome::Failed { code: 1603, retryable: false });
        assert_eq!(classify_msi(None, &gone), Outcome::Unknown(UnknownReason::ExitCodeUnavailable));
    }

    #[test]
    fn exe_outcomes_use_postconditions() {
        assert_eq!(classify_exe(Some(0), &post()), Outcome::Succeeded);
        let present = Postconditions { entry: EntryState::Present, ..post() };
        assert_eq!(classify_exe(Some(0), &present), Outcome::Unknown(UnknownReason::EntryStillPresent));
        let reboot = Postconditions { new_pending_renames: true, ..post() };
        assert_eq!(classify_exe(Some(0), &reboot), Outcome::SucceededRebootRequired);
        assert_eq!(classify_exe(Some(2), &post()), Outcome::Failed { code: 2, retryable: false });
    }

    #[test]
    fn incomplete_tracking_never_confirms_success() {
        // Parent exited 0, a child removed the entry but was still running when
        // the user stopped waiting: not a confirmed success.
        let inc = Postconditions { tracking_incomplete: true, ..post() };
        assert_eq!(classify_exe(Some(0), &inc), Outcome::Unknown(UnknownReason::TrackingIncomplete));
        let inc_reboot = Postconditions { new_pending_renames: true, ..inc.clone() };
        assert_eq!(classify_exe(Some(0), &inc_reboot), Outcome::Unknown(UnknownReason::TrackingIncomplete));
        let msi_gone = Postconditions { msi_state: Some(MsiProductState::NotInstalled), ..inc };
        assert_eq!(classify_msi(Some(0), &msi_gone), Outcome::Unknown(UnknownReason::TrackingIncomplete));
        assert_eq!(classify_msi(Some(3010), &msi_gone), Outcome::Unknown(UnknownReason::TrackingIncomplete));
        assert!(!Outcome::Unknown(UnknownReason::TrackingIncomplete).auto_scan_allowed());
        // Failures still report as failures.
        assert_eq!(classify_exe(Some(5), &msi_gone), Outcome::Failed { code: 5, retryable: false });
    }

    #[test]
    fn unreadable_uninstall_key_is_uncertain() {
        use crate::fakes::FakeReg;
        use crate::model::{RegRoot, RegView, UNINSTALL_PATH};
        let path = format!("{UNINSTALL_PATH}\\App");
        let k64 = RegKeyRef { root: RegRoot::Hklm, view: RegView::Reg64, path: path.clone() };
        let k32 = RegKeyRef { root: RegRoot::Hklm, view: RegView::Reg32, path: path.clone() };

        let mut reg = FakeReg::new();
        reg.key(RegRoot::Hklm, RegView::Reg64, &path, &[]).deny(RegRoot::Hklm, RegView::Reg64, &path);
        let st = entry_state(&reg, &[k64.clone()]);
        assert!(matches!(st, EntryState::Unreadable(_)), "denied is not absent: {st:?}");
        let post = Postconditions { entry: st, ..post() };
        assert_eq!(classify_exe(Some(0), &post), Outcome::Unknown(UnknownReason::EntryUnreadable));

        // One view absent, the other unreadable: still unreadable, not removed.
        let st = entry_state(&reg, &[k32.clone(), k64.clone()]);
        assert!(matches!(st, EntryState::Unreadable(_)));
        // One view present wins over unreadable.
        reg.key(RegRoot::Hklm, RegView::Reg32, &path, &[]);
        assert_eq!(entry_state(&reg, &[k64.clone(), k32.clone()]), EntryState::Present);
        // Truly absent everywhere.
        assert_eq!(entry_state(&FakeReg::new(), &[k64, k32]), EntryState::Absent);
    }

    #[test]
    fn uninstall_cancelled_blocks_auto_scan() {
        for o in [
            Outcome::Cancelled,
            Outcome::ElevationCancelled,
            Outcome::Failed { code: 1, retryable: false },
            Outcome::LaunchFailed { code: 2 },
            Outcome::Unknown(UnknownReason::EntryStillPresent),
        ] {
            assert!(!o.auto_scan_allowed(), "{o:?}");
            assert!(!o.cleanup_eligible(), "{o:?}");
        }
        assert!(Outcome::Succeeded.auto_scan_allowed());
        // Stage A: cleanup is never eligible, even after success.
        assert!(!Outcome::Succeeded.cleanup_eligible());
    }

    #[test]
    fn state_machine_paths() {
        let s = UninstallState::Ready.next(Event::Launch).unwrap();
        let cancelled = s.clone().next(Event::LaunchError { code: ERROR_CANCELLED }).unwrap();
        assert_eq!(
            cancelled,
            UninstallState::Finished { outcome: Outcome::ElevationCancelled, tracking_incomplete: false }
        );

        let s = s.next(Event::Started { pid: 10 }).unwrap();
        let s = s.next(Event::Tracked { pids: vec![10, 11], incomplete: true }).unwrap();
        let s = s.next(Event::Tracked { pids: vec![11], incomplete: false }).unwrap();
        assert_eq!(s, UninstallState::Running { pids: vec![11], tracking_incomplete: true }, "incomplete is sticky");
        let s = s.next(Event::Exited).unwrap();
        let s = s.next(Event::Verified(Outcome::Succeeded)).unwrap();
        assert_eq!(s, UninstallState::Finished { outcome: Outcome::Succeeded, tracking_incomplete: true });
        assert!(s.next(Event::Launch).is_err(), "finished is terminal");
        assert!(UninstallState::Ready.next(Event::Exited).is_err());
    }
}
