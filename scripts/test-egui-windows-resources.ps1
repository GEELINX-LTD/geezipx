# Check the EXE's real resources; ExtractAssociatedIcon alone can return a generic icon.
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
if (-not ('GeeZipXResourceProbe' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GeeZipXResourceProbe {
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
 static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
 static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
 [DllImport("kernel32.dll", SetLastError=true)]
 static extern uint SizeofResource(IntPtr module, IntPtr resource);
 [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
 public static void Check(string path) {
   IntPtr module = LoadLibraryEx(path, IntPtr.Zero, 0x22);
   if (module == IntPtr.Zero) throw new Exception("Cannot load EXE resources");
   try {
     foreach (int type in new int[] { 14, 3 }) {
       IntPtr resource = FindResource(module, new IntPtr(1), new IntPtr(type));
       if (resource == IntPtr.Zero || SizeofResource(module, resource) == 0)
         throw new Exception("EXE is missing embedded icon resource type " + type);
     }
   } finally { FreeLibrary(module); }
 }
}
'@
}
$path = (Resolve-Path -LiteralPath $Exe).Path
[GeeZipXResourceProbe]::Check($path)
$version = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
if ($version.ProductName -ne 'GeeZipX' -or $version.FileDescription -ne 'GeeZipX') {
  throw 'EXE product metadata does not use the GeeZipX name'
}
Write-Host 'Verified embedded EXE icon and GeeZipX product metadata'
