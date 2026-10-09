#!/bin/sh
set -eu
package=$1
case "$EXPECTED_ARCH" in amd64) expected_machine=x86_64 ;; arm64) expected_machine=aarch64 ;; *) exit 1 ;; esac
[ "$(uname -m)" = "$expected_machine" ]
if command -v openagora; then echo "Unexpected openagora on PATH" >&2; exit 1; fi
cd /packages
sha256sum -c "$(basename "$package").sha256"
case "$package" in
  *.deb) apt-get install -y "$package" ;;
  *.rpm)
    if command -v dnf >/dev/null; then
      dnf --disablerepo='*' install -y "$package"
    else
      zypper --non-interactive --no-refresh --no-gpg-checks install --no-recommends "$package"
    fi ;;
  *.pkg.tar.zst) pacman -U --noconfirm "$package" ;;
  *.apk) apk add --no-network --allow-untrusted "$package" ;;
  *) echo 'Unknown package format' >&2; exit 1 ;;
esac
[ "$(command -v openagora)" = /usr/bin/openagora ]
[ "$(openagora --version)" = "openagora $EXPECTED_VERSION" ]
openagora catalog
[ -f /usr/share/licenses/openagora/LICENSE ]
case "$package" in
  *.deb) apt-get purge -y openagora ;;
  *.rpm)
    if command -v dnf >/dev/null; then dnf --disablerepo='*' remove -y openagora
    else zypper --non-interactive --no-refresh remove openagora; fi ;;
  *.pkg.tar.zst) pacman -R --noconfirm openagora ;;
  *.apk) apk del --no-network openagora ;;
esac
[ ! -e /usr/bin/openagora ]
[ ! -e /usr/share/licenses/openagora/LICENSE ]
if command -v openagora; then echo "Unexpected openagora on PATH" >&2; exit 1; fi
printf 'Install, version, catalog and uninstall passed on %s.\n' "$expected_machine"
