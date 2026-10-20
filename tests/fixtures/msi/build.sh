#!/bin/sh
# Rebuild the reboot test MSIs on Linux with wixl (package: wixl / msitools).
set -eu
cd "$(dirname "$0")"
for n in reboot-schedule reboot-force; do
  wixl -a x64 -o "$n.msi" "$n.wxs"
done
sha256sum reboot-schedule.msi reboot-force.msi > SHA256SUMS
cat SHA256SUMS
