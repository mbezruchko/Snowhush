#!/bin/zsh
# Renders the package's logos from the app icon at the sizes the manifest's names stand for. Scaled variants would need
# a resources.pri, which the package goes without.
set -euo pipefail
cd "$(dirname "$0")"
icon=../../site/assets/icon-512.png
rm -rf Assets && mkdir Assets
sips -z 50 50 $icon --out Assets/StoreLogo.png >/dev/null
sips -z 150 150 $icon --out Assets/Square150x150Logo.png >/dev/null
sips -z 44 44 $icon --out Assets/Square44x44Logo.png >/dev/null
