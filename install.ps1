param(
    [string]$Version = $env:TERLAN_VERSION,
    [string]$InstallDir = $env:TERLAN_INSTALL_DIR,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v0.0.5"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Terlan\bin"
}

$releaseBaseUrl = $env:TERLAN_RELEASE_BASE_URL
if ([string]::IsNullOrWhiteSpace($releaseBaseUrl)) {
    $releaseBaseUrl = "https://github.com/terlan-lang/terlan/releases/download"
}

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq "Core") {
    throw "install.ps1 supports Windows only. Use install.sh on Linux or macOS."
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($architecture.ToString()) {
    "X64" {
        $terlanArch = "x86_64"
    }
    default {
        throw "unsupported Windows architecture for install.ps1: $architecture"
    }
}

$artifact = "terlc-windows-$terlanArch.zip"
$url = "$releaseBaseUrl/$Version/$artifact"

if ($DryRun -or $env:TERLAN_INSTALL_DRY_RUN -eq "1") {
    "version=$Version"
    "os=windows"
    "arch=$terlanArch"
    "artifact=$artifact"
    "url=$url"
    "install_dir=$InstallDir"
    exit 0
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("terlan-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $archive = Join-Path $tmpDir $artifact
    if ($url.StartsWith("file://")) {
        $localArtifact = ([System.Uri]$url).LocalPath
        Copy-Item -Path $localArtifact -Destination $archive -Force
    }
    else {
        Invoke-WebRequest -Uri $url -OutFile $archive
    }
    Expand-Archive -Path $archive -DestinationPath $tmpDir -Force

    $source = Join-Path $tmpDir "terlc.exe"
    if (-not (Test-Path $source)) {
        throw "release artifact $artifact did not contain terlc.exe"
    }
    $vmSource = Join-Path $tmpDir "terlan-vm.exe"
    if (-not (Test-Path $vmSource)) {
        throw "release artifact $artifact did not contain terlan-vm.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Path $source -Destination (Join-Path $InstallDir "terlc.exe") -Force
    Move-Item -Path $vmSource -Destination (Join-Path $InstallDir "terlan-vm.exe") -Force

    & (Join-Path $InstallDir "terlc.exe") --version
    & (Join-Path $InstallDir "terlan-vm.exe") --version
}
finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
