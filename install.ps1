param(
    [string]$Version = $env:TERLAN_VERSION,
    [string]$InstallDir = $env:TERLAN_INSTALL_DIR,
    [string]$LibDir = $env:TERLAN_INSTALL_LIB_DIR,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v0.0.5"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Terlan\bin"
}

if ([string]::IsNullOrWhiteSpace($LibDir)) {
    $installPrefix = Split-Path -Parent $InstallDir
    $LibDir = Join-Path $installPrefix "lib\terlan"
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
    "lib_dir=$LibDir"
    exit 0
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("terlan-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $archive = Join-Path $tmpDir $artifact
    Invoke-WebRequest -Uri $url -OutFile $archive
    Expand-Archive -Path $archive -DestinationPath $tmpDir -Force

    $source = Join-Path $tmpDir "terlc.exe"
    if (-not (Test-Path $source)) {
        throw "release artifact $artifact did not contain terlc.exe"
    }
    $runtimeSource = Join-Path $tmpDir "experimental\terlan-vm"
    if (-not (Test-Path $runtimeSource)) {
        throw "release artifact $artifact did not contain experimental\terlan-vm"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Path $source -Destination (Join-Path $InstallDir "terlc.exe") -Force

    $runtimeDestRoot = Join-Path $LibDir "experimental"
    $runtimeDest = Join-Path $runtimeDestRoot "terlan-vm"
    New-Item -ItemType Directory -Path $runtimeDestRoot -Force | Out-Null
    if (Test-Path $runtimeDest) {
        Remove-Item -Path $runtimeDest -Recurse -Force
    }
    Copy-Item -Path $runtimeSource -Destination $runtimeDest -Recurse -Force

    & (Join-Path $InstallDir "terlc.exe") --version
    & (Join-Path $InstallDir "terlc.exe") --experimental otp-runtime version
}
finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
