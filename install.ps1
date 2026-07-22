param(
    [string]$Version = $env:TERLAN_VERSION,
    [string]$InstallDir = $env:TERLAN_INSTALL_DIR,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v0.0.7"
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
    "Arm64" {
        $terlanArch = "aarch64"
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
    $checksumFile = "$archive.sha256"
    if ($url.StartsWith("file://")) {
        $localArtifact = ([System.Uri]$url).LocalPath
        Copy-Item -Path $localArtifact -Destination $archive -Force
        Copy-Item -Path "$localArtifact.sha256" -Destination $checksumFile -Force
    }
    else {
        Invoke-WebRequest -Uri $url -OutFile $archive
        Invoke-WebRequest -Uri "$url.sha256" -OutFile $checksumFile
    }
    $expectedChecksum = ((Get-Content $checksumFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actualChecksum = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualChecksum -ne $expectedChecksum) {
        throw "checksum verification failed for $artifact"
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
    $nativeWorkerSource = Join-Path $tmpDir "terlan-native-worker.exe"
    if (-not (Test-Path $nativeWorkerSource)) {
        throw "release artifact $artifact did not contain terlan-native-worker.exe"
    }
    $lspSource = Join-Path $tmpDir "terlan-lsp.exe"
    if (-not (Test-Path $lspSource)) {
        throw "release artifact $artifact did not contain terlan-lsp.exe"
    }
    $shareSource = Join-Path $tmpDir "share\terlan"
    foreach ($required in @("std", "editors\vscode", "tree-sitter-terlan", "runtime\release-self-test.tvm")) {
        if (-not (Test-Path (Join-Path $shareSource $required))) {
            throw "release artifact $artifact did not contain share/terlan/$required"
        }
    }

    $prefix = Split-Path -Parent $InstallDir
    $shareDestination = Join-Path $prefix "share\terlan"
    $compilerDestination = Join-Path $InstallDir "terlc.exe"
    $vmDestination = Join-Path $InstallDir "terlan-vm.exe"
    $nativeWorkerDestination = Join-Path $InstallDir "terlan-native-worker.exe"
    $lspDestination = Join-Path $InstallDir "terlan-lsp.exe"
    $backupDir = Join-Path $tmpDir "backup"
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    $hadCompiler = Test-Path $compilerDestination
    $hadVm = Test-Path $vmDestination
    $hadNativeWorker = Test-Path $nativeWorkerDestination
    $hadLsp = Test-Path $lspDestination
    $hadShare = Test-Path $shareDestination
    if ($hadCompiler) { Copy-Item $compilerDestination (Join-Path $backupDir "terlc.exe") }
    if ($hadVm) { Copy-Item $vmDestination (Join-Path $backupDir "terlan-vm.exe") }
    if ($hadNativeWorker) { Copy-Item $nativeWorkerDestination (Join-Path $backupDir "terlan-native-worker.exe") }
    if ($hadLsp) { Copy-Item $lspDestination (Join-Path $backupDir "terlan-lsp.exe") }
    if ($hadShare) { Copy-Item $shareDestination (Join-Path $backupDir "share") -Recurse }

    try {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        Move-Item -Path $source -Destination $compilerDestination -Force
        Move-Item -Path $vmSource -Destination $vmDestination -Force
        Move-Item -Path $nativeWorkerSource -Destination $nativeWorkerDestination -Force
        Move-Item -Path $lspSource -Destination $lspDestination -Force
        Remove-Item -Path $shareDestination -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $shareDestination -Force | Out-Null
        Copy-Item -Path (Join-Path $shareSource "*") -Destination $shareDestination -Recurse -Force
        foreach ($metadata in @("terlan-release.json", "SHA256SUMS", "terlan-install-manifest.json")) {
            Copy-Item -Path (Join-Path $tmpDir $metadata) -Destination $shareDestination -Force
        }

        & $compilerDestination --version
        & $vmDestination --version
        & $vmDestination validate-package $shareDestination
        & $nativeWorkerDestination --version
        & $lspDestination --help | Out-Null
    }
    catch {
        Remove-Item $compilerDestination, $vmDestination, $nativeWorkerDestination, $lspDestination -Force -ErrorAction SilentlyContinue
        Remove-Item $shareDestination -Recurse -Force -ErrorAction SilentlyContinue
        if ($hadCompiler) { Copy-Item (Join-Path $backupDir "terlc.exe") $compilerDestination }
        if ($hadVm) { Copy-Item (Join-Path $backupDir "terlan-vm.exe") $vmDestination }
        if ($hadNativeWorker) { Copy-Item (Join-Path $backupDir "terlan-native-worker.exe") $nativeWorkerDestination }
        if ($hadLsp) { Copy-Item (Join-Path $backupDir "terlan-lsp.exe") $lspDestination }
        if ($hadShare) { Copy-Item (Join-Path $backupDir "share") $shareDestination -Recurse }
        throw "Terlan install failed; previous installation restored. $($_.Exception.Message)"
    }
}
finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
