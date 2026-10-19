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
| Inno Setup 6.3.3 | Inno (uninstaller relaunches from a temp copy) | **passed** |

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

**Limitation of this record:** the per-app detail (outcome, number of tracked
processes, candidate lists) was not published as annotations because of a
parsing bug in the workflow step. That step is fixed in the PR that adds this
file. The next run's detail is recorded below.

## Run 2: detail run
_Pending; filled in from the run's annotations._
