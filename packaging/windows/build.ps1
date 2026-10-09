param([string]$BinaryPath, [string]$OutputDir)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Root = (Resolve-Path "$PSScriptRoot/../..").Path
if (!$OutputDir) { $OutputDir = Join-Path $Root 'dist' }
Push-Location $Root
try {
    $Version = (& python packaging/version.py).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Invalid Cargo version' }
    if (!$BinaryPath) {
        & cargo build --manifest-path engine/Cargo.toml --locked --release --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
        $BinaryPath = 'engine/target/x86_64-pc-windows-msvc/release/openagora.exe'
    }
    $BinaryPath = (Resolve-Path $BinaryPath).Path
    New-Item -ItemType Directory -Force $OutputDir | Out-Null
    $OutputDir = (Resolve-Path $OutputDir).Path
    # WiX 4 is pinned and has no commercial maintenance-fee requirement.
    # Install locally with: dotnet tool install wix --tool-path .tools/wix --version 4.0.6
    $Wix = Join-Path $Root '.tools/wix/wix.exe'
    if (!(Test-Path $Wix)) { throw 'Install WiX 4.0.6 in .tools/wix (see packaging/README.md)' }
    $env:DOTNET_ROLL_FORWARD = 'Major'
    $Msi = Join-Path $OutputDir "OpenAgora-$Version-x64.msi"
    # AUTHENTICODE HOOK: sign a staged copy of openagora.exe before wix build,
    # then sign the MSI after wix build and before generating its checksum.
    & $Wix build packaging/windows/OpenAgora.wxs -arch x64 -d "Version=$Version" -d "BinaryPath=$BinaryPath" -d "LicensePath=$Root/LICENSE" -out $Msi
    if ($LASTEXITCODE -ne 0) { throw 'WiX build failed' }
    Write-Host 'Windows MSI is unsigned; Authenticode signing is not configured.'
    & python packaging/checksums.py $Msi
    if ($LASTEXITCODE -ne 0) { throw 'Checksum generation failed' }
} finally { Pop-Location }
