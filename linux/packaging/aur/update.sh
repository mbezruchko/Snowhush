#!/usr/bin/env bash
# Brings snowhush-bin to a release once its Linux packages are up: sets the version and the tarball's checksum
# in PKGBUILD, and writes the .SRCINFO the AUR reads, ready to copy into the AUR's repository and push.
#   linux/packaging/aur/update.sh 0.6.0
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
version=${1:?usage: update.sh <version>}
tarball=https://github.com/mbezruchko/Snowhush/releases/download/v$version/snowhush-linux-x86_64.tar.gz
sum=$(curl -fsSL "$tarball" | shasum -a 256 | cut -d' ' -f1)
sed -i.old -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" -e "s/^sha256sums=.*/sha256sums=('$sum')/" "$here/PKGBUILD"
rm "$here/PKGBUILD.old"

# shellcheck source=/dev/null
source "$here/PKGBUILD"
{
    echo "pkgbase = $pkgname"
    echo "	pkgdesc = $pkgdesc"
    echo "	pkgver = $pkgver"
    echo "	pkgrel = $pkgrel"
    echo "	url = $url"
    for value in "${arch[@]}"; do echo "	arch = $value"; done
    for value in "${license[@]}"; do echo "	license = $value"; done
    for value in "${depends[@]}"; do echo "	depends = $value"; done
    for value in "${provides[@]}"; do echo "	provides = $value"; done
    for value in "${conflicts[@]}"; do echo "	conflicts = $value"; done
    for value in "${source[@]}"; do echo "	source = $value"; done
    for value in "${sha256sums[@]}"; do echo "	sha256sums = $value"; done
    echo
    echo "pkgname = $pkgname"
} > "$here/.SRCINFO"
echo "snowhush-bin $version, sha256 $sum"
