# Run before Cargo: the same publisher must be embedded into the DLL and the sparse MSIX.
param([switch]$TestCertificate, [string]$CertificateThumbprint = $env:GEEZIPX_SHELL_CERTIFICATE_THUMBPRINT)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$release = Join-Path $repo 'target/release'
New-Item -ItemType Directory -Force -Path $release | Out-Null
if ($TestCertificate) {
  if ($CertificateThumbprint -or $env:GEEZIPX_WINDOWS_CERTIFICATE_PFX) { throw 'Test signing must not use a production certificate' }
  $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=GeeZipX Native Test' -CertStoreLocation 'Cert:\CurrentUser\My' -NotAfter (Get-Date).AddDays(30)
  Export-Certificate -Cert $certificate -FilePath (Join-Path $release 'GeeZipXShell-test.cer') | Out-Null
  # This opt-in script is for disposable CI/developer profiles only. Installers never trust certificates.
  Import-Certificate -FilePath (Join-Path $release 'GeeZipXShell-test.cer') -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null
  # MSIX uses TrustedPeople; Authenticode verification of the EXE/DLL also needs a trust anchor.
  Import-Certificate -FilePath (Join-Path $release 'GeeZipXShell-test.cer') -CertStoreLocation 'Cert:\CurrentUser\Root' | Out-Null
} elseif ($CertificateThumbprint) {
  $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
} elseif ($env:GEEZIPX_WINDOWS_CERTIFICATE_PFX) {
  $pfx = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString() + '.pfx')
  try {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:GEEZIPX_WINDOWS_CERTIFICATE_PFX))
    $password = [Security.SecureString]::new()
    if ($env:GEEZIPX_WINDOWS_CERTIFICATE_PASSWORD) { $password = ConvertTo-SecureString $env:GEEZIPX_WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force }
    $certificates = @(Import-PfxCertificate -FilePath $pfx -Password $password -CertStoreLocation 'Cert:\CurrentUser\My')
    $signers = @($certificates | Where-Object HasPrivateKey)
    if ($signers.Count -ne 1) { throw 'Expected one certificate with a signing key' }
    $certificate = $signers[0]
  } finally { Remove-Item -LiteralPath $pfx -Force -ErrorAction SilentlyContinue }
} else {
  throw 'A trusted code signing certificate is required. Set GEEZIPX_WINDOWS_CERTIFICATE_PFX/PASSWORD or pass -CertificateThumbprint. Use -TestCertificate only for local/CI acceptance.'
}
if (-not $certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date)) { throw 'A valid private code signing key is required' }
if (-not $TestCertificate -and $certificate.Subject -eq $certificate.Issuer) { throw 'Production builds require a CA-issued code signing certificate' }
$env:GEEZIPX_SHELL_TEST_SIGNING = if ($TestCertificate) { '1' } else { '0' }
if (-not $TestCertificate) { Remove-Item -LiteralPath (Join-Path $release 'GeeZipXShell-test.cer') -Force -ErrorAction SilentlyContinue }
$env:GEEZIPX_SHELL_PUBLISHER = $certificate.Subject
$env:GEEZIPX_SHELL_CERTIFICATE_THUMBPRINT = $certificate.Thumbprint
if ($env:GITHUB_ENV) {
  "GEEZIPX_SHELL_TEST_SIGNING=$env:GEEZIPX_SHELL_TEST_SIGNING" >> $env:GITHUB_ENV
  "GEEZIPX_SHELL_PUBLISHER=$($certificate.Subject)" >> $env:GITHUB_ENV
  "GEEZIPX_SHELL_CERTIFICATE_THUMBPRINT=$($certificate.Thumbprint)" >> $env:GITHUB_ENV
}
Write-Host "Shell publisher: $($certificate.Subject)"
