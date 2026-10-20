# Reboot test MSIs

Small per-machine MSIs that exist only to exercise Sweep's handling of
Windows Installer reboot outcomes. Each installs one text file into
`%ProgramFiles%\<name>` and registers in Add/Remove Programs. Their reboot
actions run **only on uninstall** (`REMOVE="ALL"`).

| File | Product | ProductCode | Uninstall behaviour |
|---|---|---|---|
| `reboot-schedule.msi` | Sweep ScheduleReboot Fixture 1.0.0 | `{5E1F0A11-7C2B-4C55-9D7B-3F0A2E6B3010}` | `ScheduleReboot`: msiexec exits **3010** (restart required) |
| `reboot-force.msi` | Sweep ForceReboot Fixture 1.0.0 | `{5E1F0A11-7C2B-4C55-9D7B-3F0A2E6B1641}` | `ForceReboot`. Without `/norestart`, msiexec reboots the machine and exits **1641**. With `/norestart` (always used by Sweep), the reboot is suppressed. |

Built on Linux with `wixl` (msitools): `./build.sh`. The builds are not
byte-for-byte reproducible, so CI verifies the committed MSIs against
`SHA256SUMS`.

**Warning:** installing then uninstalling `reboot-force.msi` *outside*
Sweep, e.g. from Settings, can restart the machine. Use it only in a
disposable VM.
