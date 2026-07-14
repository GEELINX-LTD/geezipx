# GeeZipX Shell Extension — Build Development MSIX Package
#
# Builds the IExplorerCommand COM DLL, creates a sparse MSIX package, and
# signs it with a self-signed development certificate (CurrentUser\My).
#
# Usage:
#   .\build-dev-package.ps1 [-Version "0.7.5"] [-Arch "x64"]
#
# Output:
#   target/release/geezipx_shell_extension.dll
#   target/msix/GeeZipXShellExtension_<version>_<arch>_Dev.msix
#   target/msix/GeeZipXShellExtension_<version>_<arch>_Dev.cer
#   target/msix/dev-cert.pfx  (temporary, git-ignored)
#
# Prerequisites:
#   - Windows 10 SDK (MakeAppx.exe, SignTool.exe)
#   - Rust with MSVC toolchain
#   - cargo (in PATH)

param(
    [string]$Version = "0.7.5",
    [ValidateSet("x64", "arm64")]
    [string]$Arch = "x64"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path "$scriptDir\..\.."
$crateDir = "$repoRoot\crates\shell-extension"
$packageDir = "$crateDir\package"
$targetDir = "$repoRoot\target"
$msixDir = "$targetDir\msix"
$dllTargetDir = "$targetDir\release"

# --- 1. Check prerequisites --------------------------------------------------

function Find-SdkTool {
    param([string]$Name)
    # Look in standard Windows SDK paths
    $paths = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.22621.0\x64",
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.22000.0\x64",
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.20348.0\x64",
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.19041.0\x64"
    )
    # Also search for any SDK version
    $sdkBase = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    if (Test-Path $sdkBase) {
        $versions = Get-ChildItem $sdkBase -Directory | Sort-Object Name -Descending
        foreach ($v in $versions) {
            $p = Join-Path $v.FullName "x64\$Name"
            if (Test-Path $p) { $paths += $p }
        }
    }
    foreach ($p in $paths) {
        if (Test-Path $p) { return $p }
    }
    throw "Cannot find $Name in Windows SDK. Install Windows 10/11 SDK."
}

$makeappx = Find-SdkTool "MakeAppx.exe"
$signtool = Find-SdkTool "SignTool.exe"

Write-Host "[1/7] SDK tools found:" -ForegroundColor Cyan
Write-Host "  MakeAppx : $makeappx"
Write-Host "  SignTool : $signtool"

# --- 2. Build the DLL --------------------------------------------------------

Write-Host "[2/7] Building geezipx-shell-extension DLL..." -ForegroundColor Cyan
Push-Location $repoRoot
try {
    cargo build --release -p geezipx-shell-extension
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$dllPath = "$dllTargetDir\geezipx_shell_extension.dll"
if (-not (Test-Path $dllPath)) {
    throw "DLL not found at $dllPath"
}
$dllFullPath = (Resolve-Path $dllPath).Path
Write-Host "  DLL: $dllFullPath"

# --- 3. Prepare staging directory --------------------------------------------

Write-Host "[3/7] Preparing staging directory..." -ForegroundColor Cyan
$stagingDir = "$msixDir\staging"
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null

# Create assets directory (required by manifest)
$assetsDir = "$stagingDir\assets"
New-Item -ItemType Directory -Path $assetsDir -Force | Out-Null

# Generate a minimal 1×1 PNG placeholder logo (the schema requires a Logo).
# If the real icon is available, copy it instead.
$realIcon = "$repoRoot\crates\gui-tauri\src-tauri\icons\StoreLogo.png"
if (Test-Path $realIcon) {
    Copy-Item $realIcon "$assetsDir\StoreLogo.png"
    Write-Host "  Logo: copied from icons/"
} else {
    # Create a minimal valid 1×1 PNG (hex-encoded).
    $pngBytes = [Convert]::FromBase64String(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQI12P4//8/AwAI/AL+hF2oKQAAAABJRU5ErkJggg=="
    )
    [System.IO.File]::WriteAllBytes("$assetsDir\StoreLogo.png", $pngBytes)
    Write-Host "  Logo: placeholder 1x1 PNG (no real icon found)"
}

# --- 4. Substitute manifest template -----------------------------------------

Write-Host "[4/7] Generating AppxManifest.xml..." -ForegroundColor Cyan

$template = Get-Content "$packageDir\AppxManifest.xml.in" -Raw

# Build archive extension menu XML blocks
$archiveExts = @(
    ".zip", ".zipx", ".tar", ".gz", ".bz2", ".br", ".lz4", ".zst", ".xz",
    ".lzma", ".lz", ".7z", ".rar", ".cab", ".asar", ".deb", ".cpio",
    ".iso", ".udf", ".lzh", ".lha", ".zpaq", ".wim", ".isz"
)

$extMenusXml = ""
foreach ($ext in $archiveExts) {
    # Remove the leading dot for the verb ID (Windows doesn't like dots in IDs)
    $idExt = $ext.TrimStart('.')
    $extMenusXml += @"

    <desktop4:Extension Category="windows.fileExplorerContextMenus">
      <desktop4:FileExplorerContextMenus>
        <desktop4:Item Type="$ext">
          <desktop4:Verb Id="GeeZipX.ExtractHere_$idExt"
                         Clsid="{9E2B1D70-5C3F-4C6B-B2E9-8F7E6D5C4B32}" />
          <desktop4:Verb Id="GeeZipX.Extract_$idExt"
                         Clsid="{8F3A1C60-4D2E-4B5A-A1F8-7E6D5C4B3A21}" />
          <desktop4:Verb Id="GeeZipX.CompressZip_$idExt"
                         Clsid="{A0C3E4D0-6D4E-4D7C-C3F0-9A8F7E6D5C43}" />
          <desktop4:Verb Id="GeeZipX.Compress_$idExt"
                         Clsid="{B1D4F5E0-7E5F-4E8D-D4A1-0B9A8F7E6D54}" />
        </desktop4:Item>
      </desktop4:FileExplorerContextMenus>
    </desktop4:Extension>
"@
}

$packageName = "GeeZipXShellExtensionDev"
$publisher = "CN=GeeZipX Development"
$comAppId = "{E5D4C3B2-A1F0-4E9D-8C7B-6A5F4E3D2C10}"
$versionNormalized = $Version

# Normalize version to X.Y.Z.N format
$parts = $Version.Split('.')
while ($parts.Count -lt 4) { $parts += "0" }
$versionNormalized = $parts[0..3] -join '.'

$manifest = $template.
    Replace('@PACKAGE_NAME@', $packageName).
    Replace('@PUBLISHER_DN@', $publisher).
    Replace('@VERSION@', $versionNormalized).
    Replace('@ARCH@', $Arch).
    Replace('@COM_APPID@', $comAppId).
    Replace('@DLL_PATH@', $dllFullPath.Replace('\', '\\')).
    Replace('@ARCHIVE_EXT_MENUS@', $extMenusXml)

$manifestPath = "$stagingDir\AppxManifest.xml"
[System.IO.File]::WriteAllText($manifestPath, $manifest, [System.Text.Encoding]::UTF8)
Write-Host "  Manifest: $manifestPath"

# --- 5. Generate or use self-signed certificate ------------------------------

Write-Host "[5/7] Managing self-signed development certificate..." -ForegroundColor Cyan

$certSubject = "CN=GeeZipX Development"
$cert = Get-ChildItem -Path Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $certSubject -and $_.NotAfter -gt (Get-Date) } |
    Select-Object -First 1

if ($cert) {
    Write-Host "  Using existing certificate: $($cert.Thumbprint) (expires $($cert.NotAfter))"
} else {
    Write-Host "  Generating new self-signed certificate..."
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $certSubject `
        -KeyUsage DigitalSignature `
        -KeyLength 4096 `
        -CertStoreLocation Cert:\CurrentUser\My `
        -NotAfter (Get-Date).AddYears(3)

    Write-Host "  New certificate: $($cert.Thumbprint)"
}

# Export the public key as CER for distribution to TrustedPeople.
$cerPath = "$msixDir\GeeZipXShellExtension_${Version}_${Arch}_Dev.cer"
Export-Certificate -Cert $cert -FilePath $cerPath -Type CERT | Out-Null
Write-Host "  CER exported: $cerPath"

# Export PFX for SignTool (temporary, stays in target/).
$pfxPath = "$msixDir\dev-cert.pfx"
# Generate a random password per build — never hardcoded.
$pfxPlainPassword = -join ((48..57)+(65..90)+(97..122) | Get-Random -Count 32 | ForEach-Object {[char]$_})
$pfxPassword = ConvertTo-SecureString -String $pfxPlainPassword -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pfxPassword | Out-Null
Write-Host "  PFX exported: $pfxPath (temporary, git-ignored)"

# --- 6. Create MSIX package --------------------------------------------------

Write-Host "[6/7] Creating MSIX package..." -ForegroundColor Cyan

$msixPath = "$msixDir\GeeZipXShellExtension_${Version}_${Arch}_Dev.msix"

# Remove old package if present.
if (Test-Path $msixPath) { Remove-Item $msixPath -Force }

$makeappxArgs = @(
    'pack',
    '/d', $stagingDir,
    '/p', $msixPath,
    '/o'  # overwrite
)

& $makeappx @makeappxArgs
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed" }
Write-Host "  MSIX: $msixPath"

# Validate the package.
$makeappxValidateArgs = @(
    'pack',
    '/v',  # validate only
    '/d', $stagingDir,
    '/p', "$msixDir\_validate.msix"
)
& $makeappx @makeappxValidateArgs
if ($LASTEXITCODE -eq 0) {
    Remove-Item "$msixDir\_validate.msix" -Force -ErrorAction SilentlyContinue
} else {
    Write-Host "  WARNING: MakeAppx validation reported issues (may be schema-warnings)." -ForegroundColor Yellow
}

# --- 7. Sign the package -----------------------------------------------------

Write-Host "[7/7] Signing MSIX package..." -ForegroundColor Cyan

$signArgs = @(
    'sign',
    '/fd', 'SHA256',
    '/a',  # use best certificate
    '/f', $pfxPath,
    '/p', $pfxPlainPassword,
    '/tr', 'https://timestamp.digicert.com',
    '/td', 'SHA256',
    $msixPath
)

& $signtool @signArgs
if ($LASTEXITCODE -ne 0) { throw "SignTool failed" }

Write-Host ""
Write-Host "===== Build Complete =====" -ForegroundColor Green
Write-Host "DLL  : $dllFullPath"
Write-Host "MSIX : $msixPath"
Write-Host "CER  : $cerPath"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Run .\register-dev-package.ps1 to install the certificate and register the package."
Write-Host "  2. After registration, restart Explorer or log out/in to see the menu."
Write-Host "  3. Run .\unregister-dev-package.ps1 to clean up."
