# GeeZipX Shell Extension — Unregister Development Package
#
# Removes the registered sparse MSIX package and optionally the self-signed
# development certificate from CurrentUser\TrustedPeople.
#
# Usage:
#   .\unregister-dev-package.ps1 [-RemoveCert]
#
# Options:
#   -RemoveCert   After unregistering the package, ask for explicit
#                 confirmation to also remove the development certificate
#                 from Cert:\CurrentUser\TrustedPeople (by thumbprint).
#                 NEVER broadly deletes certificates.
#
# This script:
#   1. Finds and removes the GeeZipXShellExtensionDev package.
#   2. Optionally removes the matching dev certificate from TrustedPeople
#      (requires -RemoveCert + interactive confirmation).

param(
    [switch]$RemoveCert
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path "$scriptDir\..\.."
$targetDir = "$repoRoot\target"
$msixDir = "$targetDir\msix"

$packageName = "GeeZipXShellExtensionDev"
$certSubject = "CN=GeeZipX Development"

Write-Host "Unregistering GeeZipX Shell Extension development package..." -ForegroundColor Cyan

# --- Step 1: Remove the MSIX package -----------------------------------------

$pkg = Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue

if (-not $pkg) {
    Write-Host "  Package '$packageName' is not registered. Nothing to remove." -ForegroundColor Yellow
} else {
    Write-Host "  Removing package: $($pkg.PackageFullName)..."
    Remove-AppxPackage -Package $pkg.PackageFullName
    Write-Host "  Package removed."
}

# --- Step 2: Optionally remove the certificate -------------------------------

if (-not $RemoveCert) {
    Write-Host ""
    Write-Host "Certificate removal skipped. Use -RemoveCert to also clean up the cert."
    exit 0
}

# Find matching certificate in TrustedPeople.
$certs = Get-ChildItem -Path Cert:\CurrentUser\TrustedPeople |
    Where-Object { $_.Subject -eq $certSubject }

if ($certs.Count -eq 0) {
    Write-Host ""
    Write-Host "No matching certificate ($certSubject) found in TrustedPeople." -ForegroundColor Yellow
    exit 0
}

Write-Host ""
Write-Host "The following certificate(s) will be removed:" -ForegroundColor Yellow
foreach ($c in $certs) {
    Write-Host "  Thumbprint : $($c.Thumbprint)"
    Write-Host "  Subject    : $($c.Subject)"
    Write-Host "  Expires    : $($c.NotAfter)"
    Write-Host "  ---"
}

Write-Host ""
$confirm = Read-Host "Remove the above certificate(s) from TrustedPeople? (y/N)"
if ($confirm -notmatch '^[yY]') {
    Write-Host "Certificate removal skipped."
    exit 0
}

foreach ($c in $certs) {
    Remove-Item -Path $c.PSPath -Force
    Write-Host "  Removed certificate: $($c.Thumbprint)"
}

Write-Host ""
Write-Host "===== Cleanup Complete =====" -ForegroundColor Green
Write-Host "The modern context menu items should no longer appear after an Explorer restart."
Write-Host "Static verbs ('Show more options') are unaffected."
