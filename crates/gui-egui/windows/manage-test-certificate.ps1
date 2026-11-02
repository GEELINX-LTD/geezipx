# Called only after an explicit installer/uninstaller choice. No private key is shipped.
param(
  [Parameter(Mandatory)][ValidateSet('Install', 'Remove')][string]$Action,
  [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$Thumbprint,
  [string]$CertificatePath = ''
)
$ErrorActionPreference = 'Stop'
function Assert-TestCertificate([Security.Cryptography.X509Certificates.X509Certificate2]$Certificate) {
  if ($Certificate.Subject -ne 'CN=GeeZipX Native Test' -or $Certificate.Issuer -ne $Certificate.Subject -or $Certificate.HasPrivateKey) {
    throw 'Only the GeeZipX public test certificate can be managed'
  }
  $usageCount = 0
  $codeSigning = $false
  foreach ($extension in $Certificate.Extensions) {
    if ($extension.Oid.Value -eq '2.5.29.37') {
      $usageCount++
      foreach ($oid in $extension.EnhancedKeyUsages) {
        if ($oid.Value -eq '1.3.6.1.5.5.7.3.3') { $codeSigning = $true }
      }
    }
  }
  if ($usageCount -ne 1 -or -not $codeSigning) { throw 'Expected a code signing certificate' }
}
$store = $null
try {
  # Use .NET directly: a child of an installer launched by PowerShell 7 can
  # inherit module paths that prevent Windows PowerShell from loading Cert:.
  $store = [Security.Cryptography.X509Certificates.X509Store]::new(
    [Security.Cryptography.X509Certificates.StoreName]::TrustedPeople,
    [Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
  $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly)
  $existing = $store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint, $Thumbprint, $false)
  $store.Close()
  if ($Action -eq 'Install') {
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($CertificatePath)
    if ($certificate.Thumbprint -ne $Thumbprint) { throw 'Bundled certificate thumbprint mismatch' }
    if ($certificate.NotAfter -le [DateTime]::Now -or $certificate.NotBefore -gt [DateTime]::Now) { throw 'Test certificate is not currently valid' }
  } elseif ($existing.Count -ne 0) {
    $certificate = $existing[0]
  } else {
    exit 0 # Already removed: allow an interrupted uninstall to be retried.
  }
  Assert-TestCertificate $certificate
  if ($Action -eq 'Install' -and $existing.Count -ne 0) {
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
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = [IO.Path]::Combine([Environment]::SystemDirectory, 'WindowsPowerShell\v1.0\powershell.exe')
    $start.UseShellExecute = $true
    $start.Verb = 'runas'
    $start.Arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
    $process = [Diagnostics.Process]::Start($start)
    $process.WaitForExit()
    exit $process.ExitCode
  }
  $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
  $current = $store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint, $Thumbprint, $false)
  if ($Action -eq 'Install') {
    if ($current.Count -ne 0) { exit 10 } # Trust may have been imported during UAC.
    $store.Add($certificate)
    if ($store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint, $Thumbprint, $false).Count -eq 0) { throw 'Certificate import did not establish trust' }
  } elseif ($current.Count -ne 0) {
    Assert-TestCertificate $current[0]
    $store.Remove($current[0])
    if ($store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint, $Thumbprint, $false).Count -ne 0) { throw 'Certificate removal did not complete' }
  }
  exit 0
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
} finally {
  if ($null -ne $store) { $store.Close() }
}
