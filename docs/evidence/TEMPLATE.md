# Stage A VM validation record

- **Date:**
- **Tester:**
- **VM / hypervisor:**
- **Windows edition and build** (`winver`):
- **UAC level:**
- **Sweep build** (Actions run URL + commit):
- **Installer hashes verified** (yes/no):

| # | Account | Observed prompt (none / consent / credentials) | Plan text shown by Sweep | Outcome shown | Auto-scan? | Expected met? | Notes / screenshot |
|---|---|---|---|---|---|---|---|
| R1 | std1 | | | | | | |
| R2 | std1 | | | | | | |
| R3 | std1 → admin1 creds | | | | | | |
| R4 | std1 → admin1 creds | | | | | | |
| R5 | std1 (declined) | | | | | | |
| R6 | std1 → admin2 creds | | | | | | |
| R7 | std1 | | | | | | |
| R8 | | | | | | | |
| R9 | | | | | | | |
| R10 | admin1 | | | | | | |
| R11 | admin1 (elevated Sweep) | | | | | | |
| R12 | — | | | | | | |

## Leftover scan observations
For each uninstall: the candidates Sweep marked **Likely**, and whether each
really belonged to the removed program. A Likely item that belonged to
something else is a **safety failure**; record it in full.

## Deviations / failures
