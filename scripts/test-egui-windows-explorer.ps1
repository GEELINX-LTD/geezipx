# Exercise the packaged IExplorerCommand, not a test-only registry registration.
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
$build = [int](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').CurrentBuildNumber
if ($build -lt 22000) { return }
$packages = @(Get-AppxPackage -Name 'GeeZipX.Native.Shell')
if ($packages.Count -ne 1) { throw 'Expected one registered Windows 11 shell identity' }
if (-not ('GeeZipXModernAcceptance.Probe' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
namespace GeeZipXModernAcceptance {
 [ComImport, Guid("B63EA76D-1F85-456F-A19C-48159EFA858B"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface Items { }
 [ComImport, Guid("A08CE4D0-FA25-44AB-B57C-C7B1C323E0B9"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface Command {
   void GetTitle(Items items, [MarshalAs(UnmanagedType.LPWStr)] out string value);
   void GetIcon(Items items, [MarshalAs(UnmanagedType.LPWStr)] out string value);
   void GetToolTip(Items items, [MarshalAs(UnmanagedType.LPWStr)] out string value);
   void GetCanonicalName(out Guid value);
   void GetState(Items items, [MarshalAs(UnmanagedType.Bool)] bool slow, out uint value);
   void Invoke(Items items, IntPtr context);
   void GetFlags(out uint value);
   void EnumSubCommands(out Commands value);
 }
 [ComImport, Guid("A88826F8-186F-4987-AADE-EA0CEF8FBFE8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface Commands {
   [PreserveSig] int Next(uint count, [Out, MarshalAs(UnmanagedType.LPArray, SizeParamIndex=0, ArraySubType=UnmanagedType.Interface)] Command[] values, out uint fetched);
   [PreserveSig] int Skip(uint count);
   void Reset(); void Clone(out Commands value);
 }
 public static class Probe {
   [DllImport("shell32.dll", CharSet=CharSet.Unicode, PreserveSig=false)]
   static extern void SHParseDisplayName(string name, IntPtr bind, out IntPtr pidl, uint flags, out uint attributes);
   [DllImport("shell32.dll", PreserveSig=false)]
   static extern void SHCreateShellItemArrayFromIDLists(uint count, [MarshalAs(UnmanagedType.LPArray, SizeParamIndex=0)] IntPtr[] pidls, out Items items);
   public static void Check(string[] paths, bool extractable, int invoke) {
     var pidls = new IntPtr[paths.Length]; Items items = null; Command root = null; Commands iterator = null; Commands clone = null;
     var children = new Command[5]; var end = new Command[1];
     try {
       for (int i=0; i<paths.Length; i++) { uint attr; SHParseDisplayName(paths[i], IntPtr.Zero, out pidls[i], 0, out attr); }
       SHCreateShellItemArrayFromIDLists((uint)paths.Length, pidls, out items);
       root = (Command)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("5AD97452-37F8-4738-A102-5B472DF60C20"), true));
       uint state, flags; string title;
       root.GetTitle(items, out title); if (title != "GeeZipX") throw new Exception("Wrong modern menu root");
       root.GetFlags(out flags); if (flags != 1) throw new Exception("Missing subcommand flag");
       root.GetState(items, false, out state); if (state != 0) throw new Exception("Root hidden for filesystem selection");
       root.EnumSubCommands(out iterator);
       uint fetched;
       if (iterator.Next(5, children, out fetched) != 0 || fetched != 5) throw new Exception("Expected five commands");
       if (iterator.Next(1, end, out fetched) != 1 || fetched != 0) throw new Exception("Enumeration failed to end with S_FALSE");
       iterator.Clone(out clone); clone.Reset();
       if (clone.Skip(6) != 1) throw new Exception("Skip failed to end with S_FALSE");
       var names = new HashSet<Guid>();
       for (int i=0; i<5; i++) {
         Guid name; children[i].GetCanonicalName(out name); if (!names.Add(name)) throw new Exception("Duplicate command identity");
         children[i].GetTitle(items, out title); if (String.IsNullOrWhiteSpace(title)) throw new Exception("Empty command title");
         children[i].GetState(items, false, out state);
         uint expected = i < 2 || extractable ? 0u : 2u;
         if (state != expected) throw new Exception("Wrong command state at " + i + ": " + state);
       }
       if (invoke >= 0) children[invoke].Invoke(items, IntPtr.Zero);
     } finally {
       foreach (var child in children) if (child != null) Marshal.FinalReleaseComObject(child);
       foreach (var child in end) if (child != null) Marshal.FinalReleaseComObject(child);
       if (clone != null) Marshal.FinalReleaseComObject(clone);
       if (iterator != null) Marshal.FinalReleaseComObject(iterator);
       if (root != null) Marshal.FinalReleaseComObject(root);
       if (items != null) Marshal.FinalReleaseComObject(items);
       foreach (var pidl in pidls) if (pidl != IntPtr.Zero) Marshal.FreeCoTaskMem(pidl);
     }
   }
 }
}
'@
}
$temporary = Join-Path $env:RUNNER_TEMP 'GeeZipX modern shell 中文'
New-Item -ItemType Directory -Force -Path $temporary | Out-Null
$first = Join-Path $temporary '现代 空格.txt'
$second = Join-Path $temporary 'second.txt'
[IO.File]::WriteAllText($first, 'modern first selection content')
[IO.File]::WriteAllText($second, 'modern second selection content')
$output = Join-Path $temporary '现代 空格.zip'
try {
  [GeeZipXModernAcceptance.Probe]::Check([string[]]@($temporary), $false, -1)
  [GeeZipXModernAcceptance.Probe]::Check([string[]]@($first), $false, -1)
  [GeeZipXModernAcceptance.Probe]::Check([string[]]@($first, $second), $false, 1)
  $deadline = [DateTime]::UtcNow.AddSeconds(40)
  while (!(Test-Path -LiteralPath $output) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
  if (!(Test-Path -LiteralPath $output)) { throw 'Modern command produced no ZIP' }
  $zip = [IO.Compression.ZipFile]::OpenRead($output)
  try {
    if ($zip.Entries.Count -ne 2) { throw 'Modern command truncated the selection' }
    foreach ($file in @($first, $second)) {
      $entry = $zip.GetEntry([IO.Path]::GetFileName($file))
      if (!$entry) { throw "Missing modern selection entry: $file" }
      $reader = [IO.StreamReader]::new($entry.Open())
      try { if ($reader.ReadToEnd() -ne [IO.File]::ReadAllText($file)) { throw 'Modern compression changed content' } } finally { $reader.Dispose() }
    }
  } finally { $zip.Dispose() }
  [GeeZipXModernAcceptance.Probe]::Check([string[]]@($output), $true, 4)
  $destination = Join-Path $temporary '现代 空格'
  $deadline = [DateTime]::UtcNow.AddSeconds(40)
  while (!(Test-Path -LiteralPath (Join-Path $destination 'second.txt')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
  foreach ($file in @($first, $second)) {
    $extracted = Join-Path $destination ([IO.Path]::GetFileName($file))
    if (!(Test-Path -LiteralPath $extracted) -or [IO.File]::ReadAllText($extracted) -ne [IO.File]::ReadAllText($file)) { throw "Modern extraction failed: $file" }
  }
} finally {
  Get-Process -Name 'geezipx-egui' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host 'Packaged Explorer activation, command enumeration, selection state, Unicode ZIP and extraction passed'
