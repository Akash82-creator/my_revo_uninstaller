# Stage A manual VM runbook (interactive dialogs and UAC)

This is the part of Stage A validation that automation can't cover:
real uninstaller windows, UAC consent prompts and credential prompts,
standard users, and a *different* admin account. The automated part (real
MSI, NSIS and Inno uninstalls on a fresh GitHub Windows VM) is in
`.github/workflows/real-apps.yml`.

Record every run with `docs/evidence/TEMPLATE.md`, saved as
`docs/evidence/<date>-vm-<windows-build>.md`.
**A case without a filled-in record counts as not run.**

## 0. Disposable VM
1. **Create the VM:** Hyper-V (or VMware/VirtualBox) running a Windows 11
   evaluation image. Keep UAC at its default level ("Notify me only when apps
   try to make changes").
2. **Create three local accounts:**
   - `admin1` (Administrator)
   - `admin2` (Administrator)
   - `std1` (Standard user)
3. **Checkpoint:** take a checkpoint named `clean` before step 1, and revert
   to it to repeat a run.

## 1. Install Sweep and the test apps (as `admin1`)
1. **Get Sweep:** download the `sweep-stage-a` artifact from the latest green
   `windows` run on `main` (Actions tab) and run the NSIS installer. It
   installs per-machine into `C:\Program Files\Sweep`.
2. **Download the same pinned installers CI uses, then check their hashes**
   with `Get-FileHash <file> -Algorithm SHA256`:

   | File | Source | SHA-256 |
   |---|---|---|
   | `7z2408-x64.msi` | github.com/ip7z/7zip/releases/download/24.08/ | `98330e7e6db3507b444d576dc437a9ac4d82333a88a6bb6ef36a91fe3d85fa92` |
   | `npp.8.7.1.Installer.x64.exe` | github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v8.7.1/ | `aa130d151859eccc8b4cc535ae756682069d626087e8c56a35b630df4b5f0024` |
   | `innosetup-6.3.3.exe` | github.com/jrsoftware/issrc/releases/download/is-6_3_3/ | `0bcb2a409dea17e305a27a6b09555cabe600e984f88570ab72575cd7e93c95e6` |

3. **Install per-machine with the normal installer UI:**
   - 7-Zip MSI: double-click it.
   - Notepad++: run the installer normally.
   - Inno Setup: choose **"Install for all users"**.
4. **Sign in as `std1` and install Inno Setup again**, this time choosing
   **"Install for me only"**. That gives a per-user installation for `std1`.

## 2. Cases
Run the cases as `std1` unless a case says otherwise. Start Sweep from the
Start menu without "Run as administrator".

| # | Case | Steps | Expected |
|---|---|---|---|
| R1 | Sweep stays unelevated | Open Task Manager → Details → enable the "Elevated" column. | `sweep.exe` shows **No**. No admin banner in Sweep. |
| R2 | Inventory | Search for 7-Zip, Notepad++ and Inno Setup. | Machine entries show "All users". `std1`'s Inno shows "Current user". Every product appears exactly once per installation. |
| R3 | Standard user, MSI | Open 7-Zip, tick the acknowledgement, run the vendor uninstaller, and enter **admin1** credentials when asked. | The preview says Windows Installer decides elevation. A credential prompt appears (msiexec). Outcome: **Uninstaller finished**. The read-only scan starts automatically. Sweep deletes nothing. |
| R4 | Standard user, NSIS | Uninstall Notepad++ the same way and complete its wizard. | The preview's elevation text matches the plan shown. A credential prompt appears. Outcome: **Uninstaller finished**, not "uncertain", even though NSIS relaunches from `%TEMP%`. |
| R5 | UAC declined | Uninstall per-machine Inno Setup and press **No/Cancel** at the credential prompt. | Outcome: **Administrator approval was declined** (or Cancelled). **No** automatic scan. "Scan anyway" requires the checkbox. |
| R6 | Different admin account | Repeat R5 and approve with **admin2**. | Outcome: **Uninstaller finished**. The scan's AppData and Start Menu locations are **`std1`'s**, not `admin2`'s. Check the coverage list. |
| R7 | Per-user program | Uninstall `std1`'s per-user Inno Setup. | **No** UAC prompt. Outcome: **Uninstaller finished**. |
| R8 | Stop waiting | Start any remaining uninstaller and leave its wizard open. In Sweep press **Stop waiting**, then finish or cancel the wizard. | Outcome: **Outcome uncertain** ("not every uninstaller process was followed"). No automatic scan. |
| R9 | Navigate away | Start an uninstaller. While its wizard is open, click **← Programs**, then reopen it from the banner. Finish the wizard while you're on Programs. | The banner reopens the operation. After it finishes, it is listed under **Recent uninstalls** with its outcome, even though the program has gone from the list. |
| R10 | Same-user elevation | Sign in as `admin1` and uninstall something that's left. | A UAC **consent** (Yes/No) prompt appears, not a credential prompt. Outcome as in R3. |
| R11 | Elevated Sweep refused for per-user | As `admin1`, start Sweep with **Run as administrator** and try a per-user program (install one with "for me only" first). | Warning banner. The uninstall is refused with the "running elevated" message. |
| R12 | Nothing deleted by Sweep | After all cases, compare a `dir /s /b C:\Program Files` listing (and AppData) with what the vendor uninstallers removed. | No file removed that the vendor uninstaller didn't remove. There are no Sweep delete controls anywhere. |

**Not covered here:** reboot-required (3010) and reboot-initiated (1641)
outcomes. They need an MSI that requests a reboot, and they remain open
items.

## 3. After the run
Fill in the template, attach screenshots (`docs/evidence/img/<case>.png`),
and open a PR with the evidence file. Any case that fails gets an issue
describing what was observed versus what was expected.
