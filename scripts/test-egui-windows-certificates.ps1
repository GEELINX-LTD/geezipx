# Destructive certificate-store acceptance; disposable, elevated Windows CI only.
param([Parameter(Mandatory)][string]$Installer)
$ErrorActionPreference = 'Stop'
$certificateFile = Join-Path $PSScriptRoot '../target/release/GeeZipXShell-test.cer'
$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new((Resolve-Path -LiteralPath $certificateFile).Path)
$thumbprint = $certificate.Thumbprint
$store = 'Cert:\LocalMachine\TrustedPeople'
$target = "$store\$thumbprint"
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\GeeZipXNative'
$ownershipKey = "$uninstallKey\InstallerCertificates"
$installDir = Join-Path $env:RUNNER_TEMP "GeeZipX 证书 ' & `$ test"
if (Test-Path -LiteralPath $uninstallKey) { throw 'Certificate acceptance requires a clean disposable profile' }
$wasTrusted = Test-Path -LiteralPath $target
$rootBefore = @(Get-ChildItem 'Cert:\LocalMachine\Root' | ForEach-Object Thumbprint | Sort-Object)
$unrelated = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=GeeZipX Certificate Preservation Probe' -CertStoreLocation 'Cert:\CurrentUser\My'
$unrelatedFile = Join-Path $env:RUNNER_TEMP 'geezipx-unrelated.cer'
Export-Certificate -Cert $unrelated -FilePath $unrelatedFile | Out-Null
Import-Certificate -FilePath $unrelatedFile -CertStoreLocation $store | Out-Null
function Install-App([string]$Options = '', [switch]$ReuseDirectory) {
  $arguments = "/S $Options"
  if (-not $ReuseDirectory) { $arguments += " /D=$installDir" }
  $process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Certificate test installation failed: $($process.ExitCode)" }
  if ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name InstallLocation) -ne $installDir) { throw 'Upgrade did not reuse the recorded installation folder' }
  if (-not (Test-Path -LiteralPath (Join-Path $installDir 'geezipx-egui.exe'))) { throw 'Installed application is missing' }
}
function Uninstall-App([string]$Options = '') {
  # _?= waits for the real uninstaller instead of its temporary launcher.
  $process = Start-Process -FilePath (Join-Path $installDir 'uninstall.exe') -ArgumentList "/S $Options _?=$installDir" -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Certificate test uninstall failed: $($process.ExitCode)" }
  if ((Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath (Join-Path $installDir 'geezipx-egui.exe'))) { throw 'Uninstall did not finish' }
}
function Assert-Menu([bool]$Expected) {
  $menu = 'HKCU:\Software\Classes\AllFilesystemObjects\shell\GeeZipXNative'
  if ((Test-Path -LiteralPath $menu) -ne $Expected) { throw 'Menu choice was not applied' }
  $associations = 'HKCU:\Software\GeeZipXNative\Capabilities\FileAssociations'
  if ((Test-Path -LiteralPath $associations) -and (Get-Item -LiteralPath $associations).GetValueNames().Count -ne 0) { throw 'Menu-only installation changed format associations' }
}
function Assert-Owned {
  if (-not (Test-Path -LiteralPath $target)) { throw 'Opt-in did not import the test certificate' }
  if ((Get-ItemPropertyValue -LiteralPath $ownershipKey -Name $thumbprint) -ne 1) { throw 'Missing exact certificate ownership record' }
}
try {
  Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
  Install-App
  Assert-Menu $false
  if ((Test-Path -LiteralPath $target) -or (Test-Path -LiteralPath $ownershipKey)) { throw 'Default installation modified trust' }
  Uninstall-App

  # Reject a certificate request without the matching menu choice before writing files.
  $invalid = Start-Process -FilePath $Installer -ArgumentList "/S /MENU=0 /INSTALLCERT=1 /D=$installDir" -Wait -PassThru
  if ($invalid.ExitCode -eq 0 -or (Test-Path -LiteralPath $uninstallKey)) { throw 'Accepted a certificate request with the menu disabled' }

  # Pre-existing trust must survive even an explicit request to remove owned trust.
  Import-Certificate -FilePath $certificateFile -CertStoreLocation $store | Out-Null
  Install-App '/MENU=1 /INSTALLCERT=1'
  Assert-Menu $true
  if (Test-Path -LiteralPath $ownershipKey) { throw 'Installer claimed a pre-existing certificate' }
  Uninstall-App
  if (-not (Test-Path -LiteralPath $target)) { throw 'Uninstall removed pre-existing trust' }
  Remove-Item -LiteralPath $target -Force

  Install-App '/MENU=1 /INSTALLCERT=1'
  Assert-Owned
  Assert-Menu $true
  Install-App '/MENU=1 /INSTALLCERT=1' -ReuseDirectory # Upgrade without /D= must find the custom folder.
  Assert-Owned
  Assert-Menu $true
  Install-App '/MENU=0' -ReuseDirectory # Declining the menu during upgrade removes it, preserving ownership.
  Assert-Owned
  Assert-Menu $false
  Uninstall-App '/REMOVECERT=0'
  if (-not (Test-Path -LiteralPath $target)) { throw 'Explicit certificate retention was ignored' }
  Remove-Item -LiteralPath $target -Force

  Install-App '/MENU=1 /INSTALLCERT=1'
  Assert-Owned
  Uninstall-App # Default uninstall removes owned trust.
  if (Test-Path -LiteralPath $target) { throw 'Default uninstall retained owned trust' }

  Install-App '/MENU=1 /INSTALLCERT=1'
  Assert-Owned
  Remove-Item -LiteralPath $target -Force # Someone removed it before uninstall.
  Uninstall-App

  if (-not (Test-Path -LiteralPath "$store\$($unrelated.Thumbprint)")) { throw 'Removed an unrelated certificate' }
  $rootAfter = @(Get-ChildItem 'Cert:\LocalMachine\Root' | ForEach-Object Thumbprint | Sort-Object)
  if (Compare-Object $rootBefore $rootAfter) { throw 'Installer changed the trusted root store' }
  Write-Host 'Optional menu, certificate dependency, existing trust, custom-folder upgrade, ownership, explicit retention, default removal and unrelated trust passed'
} finally {
  if (Test-Path -LiteralPath (Join-Path $installDir 'uninstall.exe')) {
    $null = Start-Process -FilePath (Join-Path $installDir 'uninstall.exe') -ArgumentList "/S _?=$installDir" -Wait -PassThru
  }
  Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue
  if ($wasTrusted) {
    Import-Certificate -FilePath $certificateFile -CertStoreLocation $store | Out-Null
  } else {
    Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
  }
  Remove-Item -LiteralPath "$store\$($unrelated.Thumbprint)", "Cert:\CurrentUser\My\$($unrelated.Thumbprint)", $unrelatedFile -Force -ErrorAction SilentlyContinue
}
