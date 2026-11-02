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
function Install-App([string]$Options = '') {
  $process = Start-Process -FilePath $Installer -ArgumentList "/S $Options /D=$installDir" -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Certificate test installation failed: $($process.ExitCode)" }
}
function Uninstall-App([string]$Options = '') {
  # _?= waits for the real uninstaller instead of its temporary launcher.
  $process = Start-Process -FilePath (Join-Path $installDir 'uninstall.exe') -ArgumentList "/S $Options _?=$installDir" -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Certificate test uninstall failed: $($process.ExitCode)" }
  if ((Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath (Join-Path $installDir 'geezipx-egui.exe'))) { throw 'Uninstall did not finish' }
}
function Assert-Owned {
  if (-not (Test-Path -LiteralPath $target)) { throw 'Opt-in did not import the test certificate' }
  if ((Get-ItemPropertyValue -LiteralPath $ownershipKey -Name $thumbprint) -ne 1) { throw 'Missing exact certificate ownership record' }
}
try {
  Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
  Install-App
  if ((Test-Path -LiteralPath $target) -or (Test-Path -LiteralPath $ownershipKey)) { throw 'Default installation modified trust' }
  Uninstall-App '/REMOVECERT=1'

  # Pre-existing trust must survive even an explicit request to remove owned trust.
  Import-Certificate -FilePath $certificateFile -CertStoreLocation $store | Out-Null
  Install-App '/INSTALLCERT=1'
  if (Test-Path -LiteralPath $ownershipKey) { throw 'Installer claimed a pre-existing certificate' }
  Uninstall-App '/REMOVECERT=1'
  if (-not (Test-Path -LiteralPath $target)) { throw 'Uninstall removed pre-existing trust' }
  Remove-Item -LiteralPath $target -Force

  Install-App '/INSTALLCERT=1'
  Assert-Owned
  Install-App '/INSTALLCERT=1' # An upgrade must preserve earlier ownership.
  Assert-Owned
  Uninstall-App # Default uninstall retains trust.
  if (-not (Test-Path -LiteralPath $target)) { throw 'Default uninstall changed trust' }
  Remove-Item -LiteralPath $target -Force

  Install-App '/INSTALLCERT=1'
  Assert-Owned
  Uninstall-App '/REMOVECERT=1'
  if (Test-Path -LiteralPath $target) { throw 'Opt-in uninstall retained owned trust' }

  Install-App '/INSTALLCERT=1'
  Assert-Owned
  Remove-Item -LiteralPath $target -Force # Someone removed it before uninstall.
  Uninstall-App '/REMOVECERT=1'

  if (-not (Test-Path -LiteralPath "$store\$($unrelated.Thumbprint)")) { throw 'Removed an unrelated certificate' }
  $rootAfter = @(Get-ChildItem 'Cert:\LocalMachine\Root' | ForEach-Object Thumbprint | Sort-Object)
  if (Compare-Object $rootBefore $rootAfter) { throw 'Installer changed the trusted root store' }
  Write-Host 'Certificate opt-in, existing trust, upgrade ownership, retention, removal and unrelated trust passed'
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
