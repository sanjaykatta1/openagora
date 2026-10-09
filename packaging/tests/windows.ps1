param([Parameter(Mandatory)][string]$Msi, [switch]$TestUpgrade)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Msi = (Resolve-Path $Msi).Path
$Root = (Resolve-Path "$PSScriptRoot/../..").Path
$Version = (& python "$Root/packaging/version.py").Trim()
if ($LASTEXITCODE -ne 0) { throw 'Could not read version' }
$InstallDir = Join-Path $env:ProgramFiles 'OpenAgora'
if (Test-Path $InstallDir) { throw 'Refusing to overwrite an existing installation' }
$Hash = (Get-FileHash $Msi -Algorithm SHA256).Hash.ToLower()
if ((Get-Content "$Msi.sha256").Split(' ')[0] -ne $Hash) { throw 'MSI checksum mismatch' }
$Shell = Join-Path $PSHOME 'pwsh.exe'
$OriginalPath = $env:Path
function Invoke-Msi([string[]]$MsiArgs) {
    $p = Start-Process msiexec.exe -ArgumentList $MsiArgs -Wait -PassThru
    if ($p.ExitCode -notin @(0,3010)) { throw "msiexec failed: $($p.ExitCode)" }
}
function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')
}
$LogDir = Join-Path $Root 'dist/test-logs'
New-Item -ItemType Directory -Force $LogDir | Out-Null
try {
    Invoke-Msi @('/i', "`"$Msi`"", '/qn', '/norestart', '/l*v', "`"$LogDir/install.log`"")
    Refresh-Path
    # A fresh process must resolve the executable via the persisted PATH.
    $env:OPENAGORA_EXPECTED_VERSION = $Version
    & $Shell -NoProfile -Command '
        $ErrorActionPreference = "Stop"
        $expected = Join-Path $env:ProgramFiles "OpenAgora/openagora.exe"
        if ((Get-Command openagora).Source -ne $expected) { throw "Incorrect PATH resolution" }
        if ((openagora --version) -ne "openagora $env:OPENAGORA_EXPECTED_VERSION") { throw "Incorrect version" }
        openagora catalog
        if ($LASTEXITCODE -ne 0) { throw "Catalog failed" }
    '
    if ($LASTEXITCODE -ne 0) { throw 'New-shell install test failed' }
    $Entries = @(Get-ItemProperty 'HKLM:/Software/Microsoft/Windows/CurrentVersion/Uninstall/*' | Where-Object { $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -eq 'OpenAgora' })
    if ($Entries.Count -ne 1 -or $Entries[0].DisplayVersion -ne $Version) { throw 'Installed apps registration is incorrect' }
    Invoke-Msi @('/x', "`"$Msi`"", '/qn', '/norestart', '/l*v', "`"$LogDir/uninstall.log`"")
    if ($TestUpgrade) {
        # A synthetic older MSI tests Windows Installer upgrade semantics using
        # this build's binary. It is a test fixture, never a release artifact.
        if ([version]$Version -le [version]'0.0.0') { throw 'Upgrade fixture requires version > 0.0.0' }
        $Fixture = Join-Path $LogDir 'older.msi'
        $Payload = Join-Path $LogDir 'payload'
        Expand-Archive -Path (Join-Path (Split-Path $Msi) 'openagora-x86_64-pc-windows-msvc.zip') -DestinationPath $Payload -Force
        $env:DOTNET_ROLL_FORWARD = 'Major'
        & "$Root/.tools/wix/wix.exe" build "$Root/packaging/windows/OpenAgora.wxs" -arch x64 -d 'Version=0.0.0' -d "BinaryPath=$Payload/openagora.exe" -d "LicensePath=$Root/LICENSE" -out $Fixture
        if ($LASTEXITCODE -ne 0) { throw 'Upgrade fixture build failed' }
        Invoke-Msi @('/i', "`"$Fixture`"", '/qn', '/norestart', '/l*v', "`"$LogDir/older-install.log`"")
        Invoke-Msi @('/i', "`"$Msi`"", '/qn', '/norestart', '/l*v', "`"$LogDir/upgrade.log`"")
        $Entries = @(Get-ItemProperty 'HKLM:/Software/Microsoft/Windows/CurrentVersion/Uninstall/*' | Where-Object { $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -eq 'OpenAgora' })
        if ($Entries.Count -ne 1 -or $Entries[0].DisplayVersion -ne $Version) { throw 'Major upgrade left incorrect product registration' }
        Refresh-Path
        & $Shell -NoProfile -Command 'openagora --version; if ($LASTEXITCODE -ne 0) { throw "Upgraded command failed" }; openagora catalog; if ($LASTEXITCODE -ne 0) { throw "Upgraded catalog failed" }'
        if ($LASTEXITCODE -ne 0) { throw 'Upgraded executable failed' }
        Invoke-Msi @('/x', "`"$Msi`"", '/qn', '/norestart', '/l*v', "`"$LogDir/upgrade-uninstall.log`"")
    }
    Refresh-Path
    if (Test-Path $InstallDir) { throw 'Installer files survived uninstall' }
    if (([Environment]::GetEnvironmentVariable('Path','Machine').Split(';').TrimEnd('\')) -contains $InstallDir.TrimEnd('\')) { throw 'System PATH entry survived uninstall' }
    & $Shell -NoProfile -Command 'if (Get-Command openagora -ErrorAction SilentlyContinue) { throw "openagora survived on PATH" }'
    if ($LASTEXITCODE -ne 0) { throw 'New-shell uninstall test failed' }
} finally {
    if (Test-Path "$InstallDir/openagora.exe") { Invoke-Msi @('/x', "`"$Msi`"", '/qn', '/norestart') }
    $env:Path = $OriginalPath
}
