# Real-installer validation on a disposable GitHub Windows VM

Automated part of the Stage A validation (`.github/workflows/real-apps.yml`).
It is **not** a substitute for the manual UAC runbook (`tests/vm/RUNBOOK.md`):
- the runner is an admin account with UAC disabled;
- each vendor's silent switch is appended so the run is unattended.

## Run 1: merge of PR #4
- **Run:** https://github.com/Akash82-creator/my_revo_uninstaller/actions/runs/39322471188 (head `d918e40`)
- **VM:** GitHub-hosted `windows-latest`, fresh image
- **Installers:** pinned releases downloaded from GitHub and verified by SHA-256 before use (the hashes are in the workflow):
  - 7-Zip 24.08 MSI
  - Notepad++ 8.7.1 (NSIS)
  - Inno Setup 6.3.3 (Inno)

| App | Installer type | Result |
|---|---|---|
| 7-Zip 24.08 | MSI | **passed** |
| Notepad++ 8.7.1 | NSIS (uninstaller relaunches from `%TEMP%`) | **passed** |
| Inno Setup 6.3.3 | Inno (uninstaller relaunches from a temp copy) | ~~passed~~ **invalid** (see below) |

`test result: ok. 3 passed; 0 failed` (8.4 s for all three uninstalls).

For each app, "passed" means all of the following held:
- The app was found in the inventory.
- Its real `UninstallString` was parsed into a launchable command.
- The read-only scan before uninstalling found the installed folder with ownership evidence.
- The vendor uninstaller ran through Sweep's launch/track/verify flow, and the outcome was **Succeeded** (or SucceededRebootRequired).
- Afterwards the Uninstall registry entry was **absent** and, for the MSI, `MsiQueryProductState` reported **not installed**.
- Both scans held the safety invariants:
  - every Likely candidate has ownership evidence and no shared/user-data/coverage concerns, and is not protected;
  - every path under `C:\Windows` is Blocked;
  - `CLEANUP_ENABLED == false`.

**Weakness of run 1 (found in review):** run 1 did not reject failed
installer exit codes. It selected registrations by name prefix and installer
type only. If an install had failed on a runner that already had a matching
product, the run could have uninstalled that product and still passed. Run 1
therefore does not prove that each uninstalled registration was the one just
installed. Since then:
- the workflow fails on any non-success installer exit code (self-tested);
- the inventory is recorded before installing;
- the selected registration must match the pinned version, machine scope and
  installer type (plus the ProductCode for the MSI), and must not have
  existed before installation.

These rules have regression tests in `crates/sweep-win/tests/expect_selection.rs`.

**Confirmed on the runner (run on `dbfda81`):** the `windows-latest` image
already contains **Inno Setup 6.7.1**, registered as
`HKLM\...\Uninstall\Inno Setup 6_is1` (32-bit view). Installing the pinned
6.3.3 overwrote that registration in place. The hardened selection refused it
("already existed before installation"). Run 1's Inno Setup result therefore
**uninstalled the runner's pre-existing registration**, which is exactly the
problem the review described, and it is marked invalid above. The Inno test
now uses **WinMerge 2.16.44** (Inno-based, SHA-256 `055e9602…affeaec`), which
the image does not ship. The pre-install inventory also showed an
EXE-installed **7-Zip 26.04**. The 7-Zip MSI test is unaffected: it requires
the 24.08 ProductCode, and that registration did not exist before.

**Limitation of this record:** the per-app detail (outcome, number of tracked
processes, candidate lists) was not published as annotations because of a
parsing bug in the workflow step. That step is fixed in the PR that adds this
file. The next run's detail is recorded below.

## Run 2: hardened selection (PR #5 head `e3f58e9`)
- **Run:** https://github.com/Akash82-creator/my_revo_uninstaller/actions/runs/39325138448 (merged as `4477ec2`)
- **Installers:**
  - 7-Zip 24.08 MSI
  - Notepad++ 8.7.1 (NSIS)
  - WinMerge 2.16.44 (Inno)
  - All SHA-256 verified, all exited with code 0 (non-zero codes now fail the run).
- **Already on the runner before installing:**
  - Inno Setup 6.7.1 (EXE, machine)
  - 7-Zip 26.04 (EXE, machine)
- **Selection rules:** each test selected a registration that matched the pinned version, machine scope and installer type (plus the ProductCode for the MSI), and that did **not** exist before installation.

`test result: ok. 3 passed; 0 failed` (8.8 s).

| App | Outcome | Max processes tracked | After uninstall | Scan before (Likely / Review / Blocked) | Scan after (Likely / Review / Blocked) | Coverage after | Remaining candidates after |
|---|---|---|---|---|---|---|---|
| 7-Zip 24.08 (x64 edition), MSI | Succeeded | 1 | entry absent; MSI product not installed | 1 / 3 / 1 | 0 / 1 / 1 | 1 gap | Review: `ProgramData\...\Start Menu\Programs\7-Zip`; Blocked: `C:\Program Files\7-Zip` |
| Notepad++ (64-bit x64), NSIS | SucceededRebootRequired | 3 | entry absent | 1 / 6 / 0 | 0 / 2 / 0 | 1 gap | Review: `...\Start Menu\Programs\Notepad.lnk`; Review: `HKLM\Software\Notepad++ (32-bit)` |
| WinMerge 2.16.44.0 x64, Inno | Succeeded | 2 | entry absent | 5 / 5 / 0 | 0 / 0 / 0 | complete | none |

**Observations**
- **Descendant tracking:** this worked on real uninstallers that relaunch themselves. Sweep followed up to 3 processes for NSIS and 2 for Inno, and none of the runs had incomplete tracking.
- **Notepad++ outcome:** reported as "restart required" because the uninstaller added `PendingFileRenameOperations` entries under its install folder.
- **`C:\Program Files\7-Zip`:** it was **Blocked** both before and after the MSI uninstall. The runner's pre-existing EXE 7-Zip 26.04 registers the same folder, so Sweep protects it as another program's install location. This is the shared-folder protection working on a real system.
- **Likely leftovers after uninstall:** none in any run. Everything left after an uninstall was found by name only and stayed in Review.
- **Coverage gaps:** the 7-Zip and Notepad++ scans each recorded one gap. Which location it was is not published as an annotation; it is in the run's `real-apps-results` artifact.
- **Invariants:** they held in all six scans. Every Likely candidate had ownership evidence and no concerns, `C:\Windows` paths were Blocked, and cleanup was disabled.

## Run 3: reboot test MSIs (PR #7 head `bbc3991`, 2026-10-20)
- **Run:** https://github.com/Akash82-creator/my_revo_uninstaller/actions/runs/39398790810 (merged as `5f55808`)
- **Apps:** the same three as run 2, plus the project's own reboot test MSIs from `tests/fixtures/msi`, hash-verified against `SHA256SUMS`.

`test result: ok. 5 passed; 0 failed`. The three real apps gave the same results as run 2.

| Fixture | msiexec exit (via Sweep, with `/norestart`) | Sweep outcome | After uninstall | Machine rebooted? |
|---|---|---|---|---|
| ScheduleReboot fixture | 3010 | **SucceededRebootRequired** ("restart required") | entry absent; MSI product not installed | no |
| ForceReboot fixture | **1604** | **Failed (code 1604)** | entry absent; `MsiQueryProductState` reports not installed | no |

**Findings**
- **3010 handling works end to end.** A real Windows Installer restart request is reported as "Uninstaller finished; restart required", and the product is removed.
- **Sweep never causes a reboot:** a forced restart didn't restart the machine, because Sweep always passes `/norestart`. So outcome **1641 (restart initiated) can't occur from a Sweep-launched MSI uninstall.** It is still classified correctly by a unit test.
- **New: suppressing a `ForceReboot` makes msiexec return 1604** (`ERROR_INSTALL_SUSPEND`, "installation suspended, incomplete"). The uninstall is left suspended until the next restart, even though the registration and product state already read as removed.
  - Sweep reports this as **Failed (1604)**. That's safe: the leftover scan doesn't start automatically and nothing reads as success.
  - But it's **inaccurate**: the right wording is "uninstall suspended until restart".
  - Proposed change, not made yet: map 1604 to an uncertain outcome with a "restart required to finish" message, still without an automatic scan.

## Not covered by these runs
- Interactive uninstaller dialogs and **UAC** (standard user, other admin, consent): `tests/vm/RUNBOOK.md`, **not run yet**.
- Restart outcomes from EXE (non-MSI) uninstallers, and a real reboot followed by checking Sweep's state afterwards.
- Sweep's UI. These runs drive the same backend code the app uses, but not the Tauri front end.
