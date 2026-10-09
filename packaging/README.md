# Native packages

All native package versions come from `engine/Cargo.toml`. Python 3.11+ and
Rust (including rustup) are required. Scripts run from any working directory,
write release files to `dist/`, and fail on errors. Every distributable has a
SHA-256 sidecar containing its basename. The existing binary archives retain
their names and contents, so the curl/PowerShell installers keep working.

## Linux

Install Go and the pinned packager:

```sh
go install github.com/goreleaser/nfpm/v2/cmd/nfpm@v2.47.0
export PATH="$(go env GOPATH)/bin:$PATH"
# On a native Debian/Ubuntu runner, for Rust's static musl linker:
sudo apt-get install musl-tools
packaging/linux/build.sh amd64
# Run natively on an ARM64 Linux host:
packaging/linux/build.sh arm64
```

The optional second argument supplies an already-built static musl binary;
the third chooses an output directory. The script validates ELF architecture
and rejects a runtime interpreter. A single `linux/nfpm.yaml` produces `.deb`,
`.rpm`, `.pkg.tar.zst`, and unsigned `.apk` packages, with the MIT license and
no runtime dependencies.

## Windows

Use PowerShell on Windows x64, with Rust, Python and .NET 8 installed:

```powershell
dotnet tool install wix --tool-path .tools/wix --version 4.0.6
packaging/windows/build.ps1
```

We use the pinned WiX 4 .NET tool directly: the small XML definition makes
system PATH ownership and major upgrades explicit without coupling package
metadata to cargo-wix. WiX 4 also avoids the maintenance-fee requirements of
newer WiX versions. The tool rolls forward to .NET 8; installed OpenAgora does
not require .NET. The MSI installs per machine into Program Files, registers
in Installed apps, and removes its PATH entry when uninstalled. The stable
UpgradeCode must never change. Increment the Cargo version for major upgrades;
MSI versions must be numeric `major.minor.patch` (255/255/65535 limits).

`-BinaryPath` reuses a compiled executable; `-OutputDir` changes the destination.
The clearly marked Authenticode hook in `windows/build.ps1` is intentionally
inactive. Windows packages are unsigned today.

## macOS

With Command Line Tools, Rust and Python installed:

```sh
# Unsigned local build; installs both Rust targets if needed:
packaging/macos/build.sh
# Signed build, using your existing login keychain identities and notary profile:
export APPLE_TEAM_ID='<your team ID>'
export NOTARY_PROFILE='openagora-notary'
packaging/macos/build.sh
```

For prebuilt inputs, pass a directory containing
`aarch64-apple-darwin/openagora` and `x86_64-apple-darwin/openagora` as argument
one; an optional second argument sets the output directory.

The build combines architectures with `lipo`, signs the executable with a
Developer ID Application certificate (hardened runtime and secure timestamp),
creates a component with `pkgbuild`, and adds welcome/license screens with
`productbuild`. The distribution is signed with Developer ID Installer,
submitted with `notarytool --wait`, stapled, and assessed by Gatekeeper. The
checksum is calculated after stapling.

`macos/ci-build.sh` reads these GitHub repository secrets:

- `MACOS_CERT_P12_BASE64`: both Developer ID identities and their private keys,
  exported into one password-protected PKCS#12 file and base64 encoded.
- `MACOS_CERT_PASSWORD`: the export password.
- `APPLE_TEAM_ID`: the developer team identifier.
- `NOTARY_KEY_P8_BASE64`: the App Store Connect Team API private key, base64 encoded.
- `NOTARY_KEY_ID`: its key identifier.
- `NOTARY_ISSUER_ID`: its issuer identifier.

CI imports the identities into a temporary keychain, grants signing-tool access,
and restores the original keychain list and deletes the temporary keychain and
key files on exit, including failure. All six secrets must be configured
together. If none are present (including fork PRs), the build explicitly logs
that the package is unsigned and skips notarization; a partial configuration
fails. Never put keys, certificates, credentials, or team identifiers in this
repository. Local signing can alternatively use `NOTARY_KEY_PATH`,
`NOTARY_KEY_ID`, and `NOTARY_ISSUER_ID` instead of a keychain profile.

## Install verification and publication

The release workflow runs on PRs and on the `native-installers` development
branch, building artifacts and running clean install/run/uninstall tests. These
events cannot publish. Tag pushes and publishing workflow dispatches also run
all tests before publication. A dispatch's version must match Cargo; choose
`publish: false` for a rehearsal. The `install-tests` job blocks publication if
any platform fails or is skipped. Signed macOS builds also check signature,
Gatekeeper acceptance and stapling on both Apple Silicon and Intel runners.

Local install tests change system files. They reject an existing OpenAgora
installation rather than overwrite it:

```sh
packaging/tests/macos.sh dist/OpenAgora-0.1.0.pkg true
packaging/tests/linux.sh debian:stable amd64 dist/openagora_0.1.0-1_amd64.deb
```

On Windows, from an elevated PowerShell session:

```powershell
packaging/tests/windows.ps1 -Msi dist/OpenAgora-0.1.0-x64.msi
```

## Publishing `openagora-bin` to the AUR (manual, not performed by CI)

`arch/PKGBUILD` uses the published static tarballs, with independently pinned
SHA-256 checksums for x86_64 and aarch64. Its version and hashes describe the
existing v0.1.0 release. Update both digests from a verified new release whenever
the version changes; never use `SKIP`. The adjacent LICENSE is packaged too.

1. Create a free account at <https://aur.archlinux.org/> and add your SSH public key.
2. On Arch, install `base-devel` and `git`; review the recipe, then run
   `packaging/arch/build.sh` as a non-root user. Test installation and removal.
3. Clone `ssh://aur@aur.archlinux.org/openagora-bin.git` into a separate directory.
4. Copy `packaging/arch/PKGBUILD` and `packaging/arch/LICENSE` into that directory.
5. Run `makepkg --printsrcinfo > .SRCINFO`, then commit `PKGBUILD`, `LICENSE`, and
   `.SRCINFO` and push to the AUR remote. Do not commit binary packages.

No Homebrew, winget, Scoop or AUR publication is automated here.
