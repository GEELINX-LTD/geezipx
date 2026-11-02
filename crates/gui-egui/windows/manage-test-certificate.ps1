# Called only after an explicit installer/uninstaller choice. No private key is shipped.
param(
  [Parameter(Mandatory)][ValidateSet('Install', 'Remove')][string]$Action,
  [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$Thumbprint,
  [string]$CertificatePath
)
$ErrorActionPreference = 'Stop'
try {
  $store = 'Cert:\LocalMachine\TrustedPeople'
  $target = "$store\$Thumbprint"
  if ($Action -eq 'Install') {
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($CertificatePath)
    if ($certificate.Thumbprint -ne $Thumbprint) { throw 'Bundled certificate thumbprint mismatch' }
    if ($certificate.NotAfter -le (Get-Date) -or $certificate.NotBefore -gt (Get-Date)) { throw 'Test certificate is not currently valid' }
  } elseif (Test-Path -LiteralPath $target) {
    $certificate = Get-Item -LiteralPath $target
  } else {
    exit 0 # Already removed: allow an interrupted uninstall to be retried.
  }
  if ($certificate.Subject -ne 'CN=GeeZipX Native Test' -or $certificate.Issuer -ne $certificate.Subject -or $certificate.HasPrivateKey) {
    throw 'Only the GeeZipX public test certificate can be managed'
  }
  $usage = @($certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' })
  if ($usage.Count -ne 1 -or $usage[0].EnhancedKeyUsages.Value -notcontains '1.3.6.1.5.5.7.3.3') { throw 'Expected a code signing certificate' }
  if ($Action -eq 'Install' -and (Test-Path -LiteralPath $target)) {
    exit 10 # Pre-existing trust belongs to the user; never claim it for removal.
  }
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    # Encode the script and literal arguments so Unicode, spaces, apostrophes and
    # PowerShell metacharacters in the temporary path cannot change the command.
    $literalPath = $CertificatePath.Replace("'", "''")
    $command = "& {`n$([IO.File]::ReadAllText($PSCommandPath))`n} -Action '$Action' -Thumbprint '$Thumbprint' -CertificatePath '$literalPath'"
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $process = Start-Process -FilePath $powershell -Verb RunAs -Wait -PassThru -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $encoded)
    exit $process.ExitCode
  }
  if ($Action -eq 'Install') {
    Import-Certificate -FilePath $CertificatePath -CertStoreLocation $store | Out-Null
    if (-not (Test-Path -LiteralPath $target)) { throw 'Certificate import did not establish trust' }
  } else {
    Remove-Item -LiteralPath $target -Force
    if (Test-Path -LiteralPath $target) { throw 'Certificate removal did not complete' }
  }
  exit 0
} catch {
  Write-Error -Message $_ -ErrorAction Continue
  exit 1
}
