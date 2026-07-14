# GeeZipX Shell Extension — Register Development Package
#
# Installs the self-signed development certificate into the current user's
# TrustedPeople store and registers the sparse MSIX package.  The package
# identity must be trusted before `Add-AppxPackage` will succeed.
#
# Usage:
#   .\register-dev-package.ps1 [-MsixPath <path>] [-ExternalPath <path>] [-Force]
#
# Options:
#   -MsixPath      Path to the signed .msix file (default: auto-detect from
#                  target/msix/).
#   -ExternalPath  Directory containing geezipx-gui.exe and the DLL that the
#                  COM server will load at runtime (default: target/release/).
#   -Force         Skip the interactive confirmation prompt.  Use in CI.
#
# This script:
#   1. Imports the development certificate into Cert:\CurrentUser\TrustedPeople
#      (NOT LocalMachine\Root — self-signed root trust is dangerous).
#   2. Registers the sparse MSIX package with Add-AppxPackage, pointing
#      -ExternalLocation at the actual build/install directory.
#   3. Prints the registered package status.

param(
    [string]$MsixPath,
    [string]$ExternalPath,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path "$scriptDir\..\.."
$targetDir = "$repoRoot\target"
$msixDir = "$targetDir\msix"

# --- Resolve MSIX path -------------------------------------------------------

if (-not $MsixPath) {
    $candidates = Get-ChildItem -Path $msixDir -Filter "*.msix" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending
    if ($candidates.Count -eq 0) {
        throw "No .msix found in $msixDir. Run build-dev-package.ps1 first."
    }
    $MsixPath = $candidates[0].FullName
}

if (-not (Test-Path $MsixPath)) {
    throw "MSIX not found: $MsixPath"
}
$MsixPath = (Resolve-Path $MsixPath).Path
Write-Host "MSIX: $MsixPath" -ForegroundColor Cyan

# --- Resolve external location -----------------------------------------------

if (-not $ExternalPath) {
    $ExternalPath = "$targetDir\release"
}

if (-not (Test-Path $ExternalPath)) {
    throw "External location not found: $ExternalPath. Build the DLL and GUI first."
}
$ExternalPath = (Resolve-Path $ExternalPath).Path
Write-Host "ExternalLocation: $ExternalPath" -ForegroundColor Cyan

# --- Confirm (unless -Force) -------------------------------------------------

if (-not $Force) {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Yellow
    Write-Host "  EXPERIMENTAL PoC — DEVELOPMENT USE ONLY" -ForegroundColor Yellow
    Write-Host "============================================================" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "This will:" -ForegroundColor White
    Write-Host "  1. Add a self-signed development certificate to your" -ForegroundColor White
    Write-Host "     CURRENT USER TrustedPeople store (NOT System Root)." -ForegroundColor White
    Write-Host "  2. Register a sparse MSIX package for the Windows 11" -ForegroundColor White
    Write-Host "     modern context menu." -ForegroundColor White
    Write-Host ""
    Write-Host "Security risk: A self-signed certificate in TrustedPeople" -ForegroundColor Red
    Write-Host "allows any package signed with it to run with the declared" -ForegroundColor Red
    Write-Host "capabilities. Only proceed if you generated the certificate" -ForegroundColor Red
    Write-Host "yourself (via build-dev-package.ps1)." -ForegroundColor Red
    Write-Host ""

    $response = Read-Host "Proceed? (y/N)"
    if ($response -notmatch '^[yY]') {
        Write-Host "Aborted by user."
        exit 0
    }
}

# --- Find the certificate ----------------------------------------------------

# Find the matching CER file.
$cerPattern = "*.cer"
$cerFiles = Get-ChildItem -Path $msixDir -Filter $cerPattern -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending

if ($cerFiles.Count -eq 0) {
    throw "No .cer file found in $msixDir. Run build-dev-package.ps1 first."
}
$cerPath = $cerFiles[0].FullName
Write-Host "Certificate: $cerPath" -ForegroundColor Cyan

# Read thumbprint from the CER file (without importing yet).
$tempCert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2
$tempCert.Import($cerPath)
$thumbprint = $tempCert.Thumbprint
$subject = $tempCert.Subject
Write-Host "Thumbprint : $thumbprint"
Write-Host "Subject    : $subject"

# --- Import certificate to TrustedPeople -------------------------------------

Write-Host ""
Write-Host "Importing certificate to CurrentUser\TrustedPeople..." -ForegroundColor Cyan

# Check if already present.
$existing = Get-ChildItem -Path Cert:\CurrentUser\TrustedPeople |
    Where-Object { $_.Thumbprint -eq $thumbprint } |
    Select-Object -First 1

if ($existing) {
    Write-Host "  Certificate already present in TrustedPeople (expires $($existing.NotAfter))."
} else {
    Import-Certificate -FilePath $cerPath -CertStoreLocation Cert:\CurrentUser\TrustedPeople
    Write-Host "  Imported successfully."
}

# --- Register the package ----------------------------------------------------

Write-Host ""
Write-Host "Registering sparse MSIX package..." -ForegroundColor Cyan

# Check if already registered.
$packageName = "GeeZipXShellExtensionDev"
$existingPkg = Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue

if ($existingPkg) {
    Write-Host "  Package already registered. Removing old registration first..."
    Remove-AppxPackage -Package $existingPkg.PackageFullName
    Write-Host "  Old package removed."
}

$addArgs = @{
    Path             = $MsixPath
    ExternalLocation = $ExternalPath
    ErrorAction      = "Stop"
}

Add-AppxPackage @addArgs

Write-Host "  Package registered."

# --- Verify ------------------------------------------------------------------

Write-Host ""
Write-Host "Verifying registration..." -ForegroundColor Cyan

$registered = Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue
if ($registered) {
    Write-Host "  Package Name        : $($registered.PackageFullName)"
    Write-Host "  Install Location    : $($registered.InstallLocation)"
    Write-Host "  Status              : $($registered.Status)"
} else {
    Write-Host "  WARNING: Package not found after registration!" -ForegroundColor Red
}

Write-Host ""
Write-Host "===== Registration Complete =====" -ForegroundColor Green
Write-Host ""
Write-Host "Notes:" -ForegroundColor White
Write-Host "  - Restart Explorer (or log out/in) to see the modern context menu."
Write-Host "  - On Windows 11 22000+, items appear in the first-level context menu."
Write-Host "  - Existing static verbs remain in 'Show more options' as fallback."
Write-Host "  - To remove: run .\unregister-dev-package.ps1"
