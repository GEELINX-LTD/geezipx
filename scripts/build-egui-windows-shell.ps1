# Requires Windows SDK and the signing configuration exported BEFORE cargo build.
param([string]$Configuration = 'release')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$out = Join-Path $repo "target/$Configuration"
$dll = Join-Path $out 'geezipx_shell.dll'
$exe = Join-Path $out 'geezipx-egui.exe'
foreach ($file in @($dll, $exe)) { if (-not (Test-Path -LiteralPath $file)) { throw "Build geezipx-egui and geezipx-shell first: $file" } }
if (-not $env:GEEZIPX_SHELL_CERTIFICATE_THUMBPRINT -or -not $env:GEEZIPX_SHELL_PUBLISHER) { throw 'Run scripts/configure-egui-windows-signing.ps1 before Cargo' }
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$sdks = @(Get-ChildItem -LiteralPath $sdkRoot -Directory | Where-Object Name -Match '^\d+\.\d+\.\d+\.\d+$' | Sort-Object { [version]$_.Name } -Descending)
$sdk = $sdks | Where-Object { Test-Path (Join-Path $_.FullName 'x64/makeappx.exe') } | Select-Object -First 1
if (-not $sdk) { throw 'Windows SDK makeappx.exe not found' }
$makeappx = Join-Path $sdk.FullName 'x64/makeappx.exe'
$signtool = Join-Path $sdk.FullName 'x64/signtool.exe'
$mt = Join-Path $sdk.FullName 'x64/mt.exe'
$staging = Join-Path $out 'shell-package'
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'Assets') | Out-Null
# A sparse package deliberately refers to an external DLL and executable.
# Check the external DLL's embedded identity before /nv suppresses that package-content check.
$embedded = Join-Path $out 'shell-dll.manifest'
& $mt "-inputresource:$dll;#2" "-out:$embedded"
if ($LASTEXITCODE -ne 0) { throw 'Shell DLL has no embedded package identity' }
[xml]$identity = Get-Content -LiteralPath $embedded -Raw
$msix = $identity.SelectSingleNode("//*[local-name()='msix']")
if (-not $msix -or $msix.publisher -ne $env:GEEZIPX_SHELL_PUBLISHER -or $msix.packageName -ne 'GeeZipX.Native.Shell' -or $msix.applicationId -ne 'GeeZipXShell') { throw 'DLL and signing publisher disagree; rebuild after configuring signing' }
$metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
$version = ($metadata.packages | Where-Object name -eq 'geezipx-shell').version -replace '[-+].*$', ''
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid shell package version' }
$publisher = [Security.SecurityElement]::Escape($env:GEEZIPX_SHELL_PUBLISHER)
$manifest = (Get-Content -LiteralPath (Join-Path $repo 'crates/shell/windows/AppxManifest.xml.in') -Raw).Replace('@@VERSION@@', "$version.0").Replace('@@PUBLISHER@@', $publisher)
[IO.File]::WriteAllText((Join-Path $staging 'AppxManifest.xml'), $manifest, [Text.UTF8Encoding]::new($false))
Add-Type -AssemblyName System.Drawing
$icon = [Drawing.Icon]::new((Join-Path $repo 'crates/gui-egui/windows/icon.ico'), 256, 256)
try {
  $source = $icon.ToBitmap()
  try {
    foreach ($asset in @(@('Logo', 50), @('Square44x44Logo', 44), @('Square150x150Logo', 150))) {
      $bitmap = [Drawing.Bitmap]::new($source, [int]$asset[1], [int]$asset[1])
      try { $bitmap.Save((Join-Path $staging "Assets/$($asset[0]).png"), [Drawing.Imaging.ImageFormat]::Png) } finally { $bitmap.Dispose() }
    }
  } finally { $source.Dispose() }
} finally { $icon.Dispose() }
$package = Join-Path $out 'GeeZipXShell.msix'
& $makeappx pack /d $staging /p $package /nv /o
if ($LASTEXITCODE -ne 0) { throw 'Sparse MSIX packing failed' }
foreach ($file in @($dll, $exe, $package)) {
  $arguments = @('sign', '/fd', 'SHA256', '/sha1', $env:GEEZIPX_SHELL_CERTIFICATE_THUMBPRINT, '/s', 'My')
  if ($env:GEEZIPX_SHELL_TEST_SIGNING -ne '1') { $arguments += @('/tr', 'http://timestamp.digicert.com', '/td', 'SHA256') }
  & $signtool @arguments $file
  if ($LASTEXITCODE -ne 0) { throw "Signing failed: $file" }
  & $signtool verify /pa $file
  if ($LASTEXITCODE -ne 0) { throw "Signature verification failed: $file" }
}
Write-Host 'Signed Explorer DLL, GUI and sparse MSIX are ready'
