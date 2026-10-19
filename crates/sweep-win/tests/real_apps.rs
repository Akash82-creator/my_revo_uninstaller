//! Stage A validation against REAL third-party installers, run in a
//! disposable GitHub-hosted Windows VM (`.github/workflows/real-apps.yml`).
//!
//! The workflow installs pinned, hash-verified releases silently, then each
//! test drives Sweep's real flow: inventory → evidence snapshot → launch
//! plan → vendor uninstaller → outcome verification → read-only scan.
//!
//! One deliberate deviation from the product, for unattended runs only:
//! the vendor's *silent* switch is appended to the planned arguments
//! (`/qn`, `/S`, `/VERYSILENT`), because nobody can click the uninstaller's
//! dialogs in CI. Interactive behaviour and UAC prompts are covered by the
//! manual VM runbook (`tests/vm/RUNBOOK.md`), not here.
//!
//! Each test prints `REAL_APP_RESULT {json}` lines that the workflow turns
//! into annotations, and writes `target/real-apps/<app>.json`.
#![cfg(windows)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sweep_core::evidence::{take_snapshot, Ownership, SnapshotEnv, Verdict};
use sweep_core::inventory::{build_inventory, read_entries};
use sweep_core::launch::plan_launch;
use sweep_core::outcome::{entry_state, EntryState, Outcome, UninstallState};
use sweep_core::protect::Protection;
use sweep_core::scan::{CancelToken, ScanOptions, ScanReport, Scanner};
use sweep_win::flow::run_vendor_uninstall;
use sweep_win::fs::WinFs;
use sweep_win::known::known_folders;
use sweep_win::registry::WinReg;

const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(300);

fn summarize(r: &ScanReport) -> serde_json::Value {
    let count = |v: Verdict| r.candidates.iter().filter(|c| c.verdict == v).count();
    serde_json::json!({
        "likely": count(Verdict::Likely),
        "review": count(Verdict::Review),
        "blocked": count(Verdict::Blocked),
        "complete": r.complete,
        "gaps": r.gaps().map(|g| format!("{} ({:?})", g.location, g.status)).collect::<Vec<_>>(),
        "candidates": r.candidates.iter().map(|c| serde_json::json!({
            "target": c.target.display(),
            "verdict": format!("{:?}", c.verdict),
            "ownership": c.ownership.iter().map(|o| o.describe()).collect::<Vec<_>>(),
            "discovered_by": c.discovered_by,
            "shared": c.shared,
            "user_data": c.user_data,
            "concerns": c.concerns,
        })).collect::<Vec<_>>(),
    })
}

/// Safety invariants that must hold for every real scan.
fn check_invariants(app: &str, r: &ScanReport) {
    for c in &r.candidates {
        if c.verdict == Verdict::Likely {
            assert!(!c.ownership.is_empty(), "{app}: Likely without ownership: {}", c.target.display());
            assert!(
                c.shared.is_empty() && c.user_data.is_empty() && c.concerns.is_empty(),
                "{app}: Likely with concerns"
            );
            assert_eq!(c.protection, Protection::None, "{app}: Likely but protected");
        }
        let t = c.target.display().to_ascii_lowercase();
        if t.starts_with("c:\\windows") {
            assert_eq!(c.verdict, Verdict::Blocked, "{app}: Windows path not blocked: {t}");
        }
    }
    assert!(!sweep_core::CLEANUP_ENABLED, "Stage A must not enable cleanup");
}

fn scan(snap: &sweep_core::evidence::EvidenceSnapshot, known: &sweep_core::api::KnownFolders) -> ScanReport {
    let s =
        Scanner { snap, known, fs: &WinFs, reg: &WinReg, opts: ScanOptions::default(), cancel: CancelToken::default() };
    s.run(&mut |_| {})
}

fn real_app_cycle(app: &str, name_prefix: &str, silent: &str, expect_msi: bool) {
    let (entries, unreadable) = read_entries(&WinReg);
    let inv = build_inventory(entries, unreadable);
    // The runner image may already carry another build of the same product
    // (windows-latest ships an EXE-installed 7-Zip); pick by installer type.
    let all: Vec<_> = inv.programs.iter().filter(|p| p.display_name.starts_with(name_prefix)).collect();
    let others: Vec<String> =
        all.iter().filter(|p| p.windows_installer != expect_msi).map(|p| p.display_name.clone()).collect();
    let matches: Vec<_> = all.iter().filter(|p| p.windows_installer == expect_msi).collect();
    assert_eq!(
        matches.len(),
        1,
        "{app}: expected one {} entry starting with {name_prefix:?}, found {:?}",
        if expect_msi { "MSI" } else { "non-MSI" },
        all.iter().map(|p| (&p.display_name, p.windows_installer)).collect::<Vec<_>>()
    );
    let prog = *matches[0];

    let known = known_folders();
    let env = |n: &str| std::env::var(n).ok();
    let snap = take_snapshot(prog, &inv, &SnapshotEnv { known: &known, fs: &WinFs, env: &env });
    let cmd = snap
        .command
        .clone()
        .unwrap_or_else(|e| panic!("{app}: uninstall command unsupported: {e} ({:?})", prog.uninstall_string));

    // Read-only analysis while installed: the install folder is found with ownership evidence.
    let before = scan(&snap, &known);
    check_invariants(app, &before);
    if let Ok(dir) = &snap.install_dir {
        let c = before.candidates.iter().find(|c| c.ownership.contains(&Ownership::IsInstallDir));
        assert!(c.is_some(), "{app}: installed folder {dir} not found by pre-uninstall scan");
    }

    let elevated = sweep_win::system::is_elevated();
    let mut plan = plan_launch(
        &cmd,
        prog.scope,
        elevated,
        sweep_win::manifest::execution_level_for(&cmd),
        known.system32.as_ref(),
    )
    .unwrap_or_else(|e| panic!("{app}: launch refused: {e}"));
    let planned_params = plan.params.clone();
    // Test-only deviation: unattended run.
    plan.params = format!("{} {silent}", plan.params).trim().to_string();

    let stop = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < UNINSTALL_TIMEOUT && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(500));
            }
            stop.store(true, Ordering::SeqCst);
        })
    };
    let started = Instant::now();
    let mut max_tracked = 0usize;
    let state = run_vendor_uninstall(&snap, &plan, &stop, |s| {
        if let UninstallState::Running { pids, .. } = s {
            max_tracked = max_tracked.max(pids.len());
        }
    });
    let secs = started.elapsed().as_secs_f32();
    stop.store(true, Ordering::SeqCst);
    let _ = watchdog.join();

    let (outcome, incomplete) = match &state {
        UninstallState::Finished { outcome, tracking_incomplete } => (outcome.clone(), *tracking_incomplete),
        other => panic!("{app}: not finished: {other:?}"),
    };
    let entry = entry_state(&WinReg, &snap.uninstall_keys);
    let msi_state = snap.product_code.as_deref().filter(|_| expect_msi).map(sweep_win::system::msi_product_state);

    let after = scan(&snap, &known);
    check_invariants(app, &after);

    let result = serde_json::json!({
        "app": app,
        "display_name": prog.display_name,
        "other_builds_installed": others,
        "version": prog.version,
        "scope": format!("{:?}", prog.scope),
        "registry_views": prog.sources.iter().map(|s| format!("{:?}", s.view)).collect::<Vec<_>>(),
        "uninstall_string": prog.uninstall_string,
        "parsed_command": format!("{cmd:?}"),
        "install_dir": snap.install_dir.as_ref().map(|d| d.to_string()).map_err(|e| format!("{e:?}")),
        "sweep_elevated": elevated,
        "planned_verb": format!("{:?}", plan.verb),
        "planned_params": planned_params,
        "silent_switch_added_for_ci": silent,
        "outcome": format!("{outcome:?}"),
        "tracking_incomplete": incomplete,
        "max_tracked_processes": max_tracked,
        "seconds": secs,
        "entry_after": format!("{entry:?}"),
        "msi_state_after": msi_state.map(|s| format!("{s:?}")),
        "scan_before": summarize(&before),
        "scan_after": summarize(&after),
    });
    let line = serde_json::to_string(&result).unwrap();
    println!("REAL_APP_RESULT {line}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/real-apps");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{app}.json")), serde_json::to_string_pretty(&result).unwrap()).unwrap();

    assert!(
        matches!(outcome, Outcome::Succeeded | Outcome::SucceededRebootRequired),
        "{app}: outcome {outcome:?} (tracking incomplete: {incomplete})"
    );
    assert_eq!(entry, EntryState::Absent, "{app}: Uninstall entry after uninstall");
    if expect_msi {
        assert_eq!(msi_state, Some(sweep_core::msi::MsiProductState::NotInstalled), "{app}: MSI state");
    }
}

#[test]
#[ignore = "real-apps workflow only: needs the pinned installers to be installed first"]
fn msi_7zip() {
    real_app_cycle("7zip-msi", "7-Zip", "/qn", true);
}

#[test]
#[ignore = "real-apps workflow only: needs the pinned installers to be installed first"]
fn nsis_notepadplusplus() {
    // NSIS uninstallers copy themselves to %TEMP% and relaunch: exercises
    // descendant tracking against a real vendor uninstaller.
    real_app_cycle("notepadpp-nsis", "Notepad++", "/S", false);
}

#[test]
#[ignore = "real-apps workflow only: needs the pinned installers to be installed first"]
fn inno_innosetup() {
    // Inno Setup uninstallers also relaunch from a temp copy.
    real_app_cycle("innosetup-inno", "Inno Setup", "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART", false);
}
