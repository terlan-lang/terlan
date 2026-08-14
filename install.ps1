param(
    [string]$Version = $env:TERLAN_VERSION,
    [string]$InstallDir = $env:TERLAN_INSTALL_DIR,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
if ($PSVersionTable.PSEdition -eq "Desktop") {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
}

function Invoke-TerlanDownload {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    $parsed = [System.Uri]$Uri
    if ($parsed.IsFile) {
        Copy-Item -Path $parsed.LocalPath -Destination $Destination -Force
        return
    }
    for ($attempt = 1; $attempt -le 4; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Uri -OutFile $Destination -TimeoutSec 600 -UseBasicParsing
            return
        }
        catch {
            if ($attempt -eq 4) {
                throw
            }
            Start-Sleep -Seconds (2 * $attempt)
        }
    }
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v0.0.7"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Terlan\bin"
}

if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:[-.][0-9A-Za-z.-]+)?$') {
    throw "invalid Terlan release version: $Version"
}

$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)
$installRoot = [System.IO.Path]::GetPathRoot($InstallDir)
if ($InstallDir.TrimEnd('\') -eq $installRoot.TrimEnd('\')) {
    throw "TERLAN_INSTALL_DIR must not be a filesystem root: $InstallDir"
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
$releaseUri = [System.Uri]$url
if ($releaseUri.Scheme -notin @("https", "file")) {
    throw "release URL must use https:// or file://: $url"
}

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
    Invoke-TerlanDownload -Uri $url -Destination $archive
    Invoke-TerlanDownload -Uri "$url.sha256" -Destination $checksumFile
    $checksumRows = @(Get-Content $checksumFile)
    if ($checksumRows.Count -ne 1 -or $checksumRows[0] -notmatch '^([0-9a-fA-F]{64})  ([^/\\]+)$' -or $Matches[2] -ne $artifact) {
        throw "invalid SHA-256 file for $artifact"
    }
    $expectedChecksum = $Matches[1].ToLowerInvariant()
    $actualChecksum = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualChecksum -ne $expectedChecksum) {
        throw "checksum verification failed for $artifact"
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archiveHandle = [System.IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $destinationRoot = [System.IO.Path]::GetFullPath($tmpDir).TrimEnd('\') + '\'
        $archivePaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $archiveHandle.Entries) {
            $destination = [System.IO.Path]::GetFullPath((Join-Path $tmpDir $entry.FullName))
            if (-not $destination.StartsWith($destinationRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "release artifact contains an unsafe path: $($entry.FullName)"
            }
            if (-not $archivePaths.Add($destination)) {
                throw "release artifact contains a duplicate path: $($entry.FullName)"
            }
            $unixFileType = (($entry.ExternalAttributes -shr 16) -band 0xF000)
            if ($unixFileType -eq 0xA000) {
                throw "release artifact contains a symbolic link: $($entry.FullName)"
            }
            if ($unixFileType -notin @(0, 0x4000, 0x8000)) {
                throw "release artifact contains a special filesystem entry: $($entry.FullName)"
            }
        }
    }
    finally {
        $archiveHandle.Dispose()
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
    $internalChecksums = Join-Path $tmpDir "SHA256SUMS"
    if (-not (Test-Path $internalChecksums -PathType Leaf)) {
        throw "release artifact $artifact did not contain SHA256SUMS"
    }
    $checksumPaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $checksumCount = 0
    foreach ($line in Get-Content $internalChecksums) {
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            throw "release artifact contains an invalid SHA256SUMS row"
        }
        $relative = $Matches[2].Replace('/', [System.IO.Path]::DirectorySeparatorChar)
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $tmpDir $relative))
        $root = [System.IO.Path]::GetFullPath($tmpDir).TrimEnd('\') + '\'
        if (-not $candidate.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "SHA256SUMS contains an unsafe path: $relative"
        }
        if (-not (Test-Path $candidate -PathType Leaf)) {
            throw "SHA256SUMS references a missing file: $relative"
        }
        if (-not $checksumPaths.Add($relative)) {
            throw "SHA256SUMS contains a duplicate path: $relative"
        }
        $internalActual = (Get-FileHash -Path $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($internalActual -ne $Matches[1].ToLowerInvariant()) {
            throw "internal checksum verification failed for $relative"
        }
        $checksumCount++
    }
    if ($checksumCount -eq 0) {
        throw "release artifact contains an empty SHA256SUMS manifest"
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
