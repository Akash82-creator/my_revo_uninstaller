# Evidence

Records of validation runs. A test plan or script without a record here
counts as **not run**.

| Record | Kind | Status |
|---|---|---|
| `2026-10-19-ci-real-apps.md` | Automated: real MSI (7-Zip), NSIS (Notepad++) and Inno (WinMerge) uninstalls on a disposable GitHub Windows VM | **Run 2 passed** (3/3, hardened selection). Run 1's Inno result is invalid. |
| `<date>-vm-<build>.md` | Manual: interactive dialogs and UAC (`tests/vm/RUNBOOK.md`, cases R1–R12) | **not run yet** |
| `<date>-reboot-<build>.md` | Manual: Windows Installer reboot outcomes, exit codes 3010 and 1641 | **not run yet**; no procedure written yet |

## Stage B gate
Stage B (Sweep's own cleanup) stays **disabled** (`sweep_core::CLEANUP_ENABLED = false`).
It may not be enabled until every item below has a record in this folder that
shows it passed, in addition to Stage B's own safety requirements
(`docs/journal.md`, `docs/helper-security.md`):

- [x] Real MSI/NSIS/Inno uninstalls on a disposable VM (automated, unattended): `2026-10-19-ci-real-apps.md`, Run 2
- [ ] Interactive dialogs and UAC: standard user, other admin, consent, declined, per-user, elevated-Sweep refusal (R1–R12)
- [ ] Reboot outcomes: 3010 (restart required) and 1641 (restart initiated)
