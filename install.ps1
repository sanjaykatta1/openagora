# Installs the `openagora` command on Windows:
#
#   irm https://raw.githubusercontent.com/sanjaykatta1/openagora/main/install.ps1 | iex
#
# Settings (environment variables):
#   OPENAGORA_VERSION      release tag to install, e.g. v0.1.0 (default: latest)
#   OPENAGORA_INSTALL_DIR  where to put the binary (default: %LOCALAPPDATA%\OpenAgora\bin)
#   OPENAGORA_BASE_URL     download from this URL instead of GitHub releases

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    $repo = 'sanjaykatta1/openagora'
    $version = if ($env:OPENAGORA_VERSION) { $env:OPENAGORA_VERSION } else { 'latest' }
    $installDir = if ($env:OPENAGORA_INSTALL_DIR) { $env:OPENAGORA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'OpenAgora\bin' }

    # Only an x64 build is published; Windows on ARM runs it through emulation.
    $asset = 'openagora-x86_64-pc-windows-msvc.zip'
    $base = if ($env:OPENAGORA_BASE_URL) { $env:OPENAGORA_BASE_URL }
            elseif ($version -eq 'latest') { "https://github.com/$repo/releases/latest/download" }
            else { "https://github.com/$repo/releases/download/$version" }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "openagora: downloading $asset ($version)"
        Invoke-WebRequest -UseBasicParsing "$base/$asset" -OutFile (Join-Path $tmp $asset)
        Invoke-WebRequest -UseBasicParsing "$base/$asset.sha256" -OutFile (Join-Path $tmp "$asset.sha256")

        $expected = ((Get-Content (Join-Path $tmp "$asset.sha256") -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash (Join-Path $tmp $asset) -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) { throw "checksum mismatch for $asset; nothing was installed" }

        Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        Copy-Item (Join-Path $tmp 'openagora.exe') (Join-Path $installDir 'openagora.exe') -Force

        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not (($userPath -split ';') -contains $installDir)) {
            $newPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            Write-Host "openagora: added $installDir to your PATH; open a new terminal to use it."
        }
        $installed = & (Join-Path $installDir 'openagora.exe') --version
        Write-Host "openagora: installed $installed at $installDir\openagora.exe"
        Write-Host 'openagora: try: openagora catalog'
    }
    finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}
