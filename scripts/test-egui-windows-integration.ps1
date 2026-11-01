# Native acceptance for the installed egui binary. Runs in the disposable CI user profile.
param([Parameter(Mandatory)][string]$Exe, [switch]$Uninstalled)
$ErrorActionPreference = 'Stop'
$classes = 'HKCU:\Software\Classes'
$menu = "$classes\AllFilesystemObjects\shell\GeeZipXNative"
$capabilities = 'HKCU:\Software\GeeZipXNative\Capabilities'
$clsids = @('{5AD97452-37F8-4738-A102-5B472DF60C11}', '{5AD97452-37F8-4738-A102-5B472DF60C12}')
if ($Uninstalled) {
  if (Test-Path -LiteralPath 'HKCU:\Software\GeeZipXNative\Shell') { throw 'Modern shell preferences survived uninstall' }
  if (@(Get-AppxPackage -Name 'GeeZipX.Native.Shell').Count -ne 0) { throw 'Modern shell identity survived uninstall' }
  foreach ($path in @($menu, $capabilities, "$classes\GeeZipX.Native.zip") + @($clsids | ForEach-Object { "$classes\CLSID\$_" })) {
    if (Test-Path -LiteralPath $path) { throw "Integration survived uninstall: $path" }
  }
  $registered = Get-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -ErrorAction SilentlyContinue
  if ($registered.GeeZipX -or $registered.'GeeZipX Native') { throw 'RegisteredApplications survived uninstall' }
  Write-Host 'Native shell integration uninstall passed'
  return
}
./scripts/test-egui-windows-resources.ps1 -Exe $Exe
function Invoke-Integration([string]$Action) {
  $process = Start-Process -FilePath $Exe -ArgumentList $Action -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "$Action failed: $($process.ExitCode)" }
}
# UserChoice is a protected Windows setting; registration must leave it intact.
$choicePath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.zip\UserChoice'
$before = Get-ItemProperty -LiteralPath $choicePath -ErrorAction SilentlyContinue
Invoke-Integration '--register-shell-integration'
Invoke-Integration '--register-shell-integration'
$after = Get-ItemProperty -LiteralPath $choicePath -ErrorAction SilentlyContinue
if ((Get-ItemProperty -LiteralPath $capabilities).ApplicationName -ne 'GeeZipX') { throw 'Wrong default-app display name' }
$registered = Get-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications'
if (-not $registered.GeeZipX -or $registered.'GeeZipX Native') { throw 'Default-app name migration failed' }
if ($before.ProgId -ne $after.ProgId -or $before.Hash -ne $after.Hash) { throw 'Registration changed UserChoice' }
$command = (Get-Item -LiteralPath "$classes\GeeZipX.Native.zip\shell\open\command").GetValue('')
if ($command -ne "`"$Exe`" -- `"%1`"") { throw "Wrong archive launch command: $command" }
foreach ($verb in @('compress', 'compress-zip', 'extract', 'extract-here', 'extract-folder')) {
  if (!(Test-Path -LiteralPath "$menu\shell\$verb\command")) { throw "Missing menu action: $verb" }
}
foreach ($clsid in $clsids) {
  $server = Get-Item -LiteralPath "$classes\CLSID\$clsid\LocalServer32"
  if ($server.GetValue('') -ne "`"$Exe`"") { throw 'COM must append its own -Embedding argument' }
  if ($server.GetValue('ServerExecutable') -ne $Exe) { throw 'Wrong COM executable' }
}
if ((Get-Item -LiteralPath "$menu\shell\compress").GetValue('MultiSelectModel') -ne 'Player') { throw 'Compression does not support a complete selection' }
# Another application's OpenWith value must survive our remove/register cycle.
$openWith = "$classes\.zip\OpenWithProgids"
New-ItemProperty -LiteralPath $openWith -Name 'GeeZipXIntegrationTest.Other' -Value '' -Force | Out-Null
try {
  Invoke-Integration '--remove-shell-integration'
  if ((Get-Item -LiteralPath $openWith).GetValueNames() -notcontains 'GeeZipXIntegrationTest.Other') { throw 'Removed another handler' }
  Invoke-Integration '--repair-shell-integration' # A clean install stays opt-in.
  if (Test-Path -LiteralPath $menu) { throw 'Repair re-enabled removed integration' }
  Invoke-Integration '--register-shell-integration'
  Invoke-Integration '--repair-shell-integration'
} finally {
  Remove-ItemProperty -LiteralPath $openWith -Name 'GeeZipXIntegrationTest.Other' -ErrorAction SilentlyContinue
}
# Activate the real DelegateExecute COM local server with two selected Unicode files.
if (-not ('GeeZipXShellProbe' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("7F9185B0-CB92-43C5-80A9-92277A4F7B54"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IExecuteCommand {
 void SetKeyState(uint value); void SetParameters([MarshalAs(UnmanagedType.LPWStr)] string value);
 void SetPosition(ref NativePoint value); void SetShowWindow(int value);
 void SetNoShowUI([MarshalAs(UnmanagedType.Bool)] bool value);
 void SetDirectory([MarshalAs(UnmanagedType.LPWStr)] string value); void Execute();
}
[StructLayout(LayoutKind.Sequential)] struct NativePoint { public int X; public int Y; }
[ComImport, Guid("B63EA76D-1F85-456F-A19C-48159EFA858B"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IShellItemArray { }
[ComImport, Guid("1C9CD5BB-98E9-4491-A60F-31AACC72B83C"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IObjectWithSelection {
 void SetSelection([MarshalAs(UnmanagedType.Interface)] IShellItemArray selection);
 void GetSelection(ref Guid iid, out IntPtr selection);
}
public static class GeeZipXShellProbe {
 [DllImport("shell32.dll", CharSet=CharSet.Unicode, PreserveSig=false)]
 static extern void SHParseDisplayName(string name, IntPtr bind, out IntPtr pidl, uint flags, out uint attributes);
 [DllImport("shell32.dll", PreserveSig=false)]
 static extern void SHCreateShellItemArrayFromIDLists(uint count, [MarshalAs(UnmanagedType.LPArray, SizeParamIndex=0)] IntPtr[] pidls, [MarshalAs(UnmanagedType.Interface)] out IShellItemArray items);
 public static void Execute(string clsid, string[] paths) {
   var pidls = new IntPtr[paths.Length]; IShellItemArray items = null; object command = null;
   try {
     for (int i=0; i<paths.Length; i++) { uint attr; SHParseDisplayName(paths[i], IntPtr.Zero, out pidls[i], 0, out attr); }
     SHCreateShellItemArrayFromIDLists((uint)pidls.Length, pidls, out items);
     command = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid(clsid), true));
     ((IObjectWithSelection)command).SetSelection(items);
     ((IExecuteCommand)command).Execute();
   } finally {
     if (command != null) Marshal.FinalReleaseComObject(command);
     if (items != null) Marshal.FinalReleaseComObject(items);
     foreach (var pidl in pidls) if (pidl != IntPtr.Zero) Marshal.FreeCoTaskMem(pidl);
   }
 }
}
'@
}
$temporary = Join-Path $env:RUNNER_TEMP 'GeeZipX shell 中文 selection'
New-Item -ItemType Directory -Path $temporary -Force | Out-Null
$first = Join-Path $temporary '说明 空格.txt'
$second = Join-Path $temporary 'second.txt'
[IO.File]::WriteAllText($first, 'first selection content')
[IO.File]::WriteAllText($second, 'second selection content')
$output = Join-Path $temporary '说明 空格.zip'
try {
  [GeeZipXShellProbe]::Execute($clsids[1], [string[]]@($first, $second))
  $deadline = [DateTime]::UtcNow.AddSeconds(40)
  while (!(Test-Path -LiteralPath $output) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
  if (!(Test-Path -LiteralPath $output)) { throw 'COM compression produced no archive' }
  # The final output is installed atomically; it is complete when it becomes visible.
  $zip = [IO.Compression.ZipFile]::OpenRead($output)
  try {
    if ($zip.Entries.Count -ne 2) { throw 'Explorer multi-selection was truncated' }
    foreach ($name in @('说明 空格.txt', 'second.txt')) {
      $entry = $zip.GetEntry($name)
      if (!$entry) { throw "Missing selected file: $name" }
      $reader = [IO.StreamReader]::new($entry.Open())
      try { if (!$reader.ReadToEnd().Contains('selection content')) { throw "Corrupt file: $name" } } finally { $reader.Dispose() }
    }
  } finally { $zip.Dispose() }
} finally {
  Get-Process -Name 'geezipx-egui' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
& "$PSScriptRoot/test-egui-windows-explorer.ps1" -Exe $Exe
Write-Host 'Native file associations, registry ownership and COM multi-selection passed'
