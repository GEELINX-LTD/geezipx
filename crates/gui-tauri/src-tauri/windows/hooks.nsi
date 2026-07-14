; GeeZipX NSIS installer hooks
; =====================================================================
; Strategy (v0.7.5+)
; ------------------
; Registry keys are written to HKCU\Software\Classes (per-user, no
; admin required). This matches the runtime `set_shell_menu` Tauri
; command, which also writes to HKCU, so the Settings page can
; dynamically toggle verbs without fighting the installer.
;
; Tauri NSIS defaults to `installMode: "currentUser"`, so HKCU is
; always the correct hive. If `installMode` is changed to `perMachine`
; the installer runs elevated and HKCU would point to the elevated
; user — this is a known NSIS limitation. The current tauri.conf.json
; does not set installMode, so the default applies.
;
; Sentinel
; --------
; After every successful `set_shell_menu` call the runtime writes:
;   HKCU\Software\Classes\GeeZipX\ShellMenu  Configured=1
;
; preInstall checks this sentinel FIRST. If present, the user has
; deliberately configured the shell menu (even to "off") — skip all
; registration. As a fallback for users who ran the runtime before the
; sentinel was introduced, we also check for existing verb keys and
; treat them as evidence of prior configuration.
;
; On a fresh install (no sentinel, no old verbs) the installer
; registers all four verbs as defaults AND writes the sentinel so
; future upgrades preserve the user's choices.
;
; Uninstall
; ----------
; preUnInstall cleans:
;   1. Sentinel key
;   2. HKCU\Software\Classes verb keys (current location)
;   3. HKCR verb keys (legacy v0.7.4 and earlier)
;
; Verbs registered (PascalCase key suffixes)
; ------------------------------------------
;   Archive files (.zip / .7z / .rar / ...):
;     - Extract here      (GeeZipX.ExtractHere)  → /extract-here "%1"
;     - Extract to...     (GeeZipX.Extract)       → /extract "%1"
;
;   All files (*) and directories:
;     - Compress as ZIP   (GeeZipX.CompressZip)   → /compress-zip "%1"
;     - Compress as...    (GeeZipX.Compress)      → /compress "%1"
;
; Labels are in English (matching the runtime) so toggling via the
; Settings page does not leave stale localised strings.
;
; On Windows 11 these appear under "Show more options" (classic menu)
; — this is a system limitation.
; =====================================================================

!macro AddExtractMenus ext
  ; ── "Extract here" (top) ──
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "" "Extract here"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "MUIVerb" "Extract here"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere\command" "" '"$INSTDIR\geezipx-gui.exe" /extract-here "%1"'

  ; ── "Extract to..." (bottom) ──
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "" "Extract to..."
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "MUIVerb" "Extract to..."
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract\command" "" '"$INSTDIR\geezipx-gui.exe" /extract "%1"'
!macroend

!macro RemoveExtractMenus ext
  ; ── HKCU (current location) ──
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX"
  DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${ext}\shell"
  DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${ext}"

  ; ── HKCR (legacy v0.7.4 and earlier) ──
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell\GeeZipX"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}"
!macroend

!macro preInstall
  ; ===================================================================
  ; Step 1 — Check sentinel. If the runtime has ever written the
  ; sentinel, the user has made a deliberate choice. Skip everything.
  ; ===================================================================
  ReadRegStr $0 HKCU "Software\Classes\GeeZipX\ShellMenu" "Configured"
  ${If} $0 == "1"
    Goto skip_all_menus
  ${EndIf}

  ; ===================================================================
  ; Step 2 — Fallback: check for existing verb keys (pre-sentinel
  ; users). If ANY GeeZipX verb key exists under HKCU, treat it as
  ; evidence of prior configuration. Write the sentinel so future
  ; upgrades also skip, then bail out.
  ; ===================================================================
  ReadRegStr $0 HKCU "Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX.ExtractHere" ""
  ${If} $0 != ""
    WriteRegStr HKCU "Software\Classes\GeeZipX\ShellMenu" "Configured" "1"
    Goto skip_all_menus
  ${EndIf}

  ; ===================================================================
  ; Step 3 — Fresh install. Register all four verbs and write the
  ; sentinel.
  ; ===================================================================

  ; Extract menus — one per supported archive extension
  !insertmacro AddExtractMenus ".zip"
  !insertmacro AddExtractMenus ".zipx"
  !insertmacro AddExtractMenus ".tar"
  !insertmacro AddExtractMenus ".gz"
  !insertmacro AddExtractMenus ".bz2"
  !insertmacro AddExtractMenus ".br"
  !insertmacro AddExtractMenus ".lz4"
  !insertmacro AddExtractMenus ".zst"
  !insertmacro AddExtractMenus ".xz"
  !insertmacro AddExtractMenus ".lzma"
  !insertmacro AddExtractMenus ".lz"
  !insertmacro AddExtractMenus ".7z"
  !insertmacro AddExtractMenus ".rar"
  !insertmacro AddExtractMenus ".cab"
  !insertmacro AddExtractMenus ".asar"
  !insertmacro AddExtractMenus ".deb"
  !insertmacro AddExtractMenus ".cpio"
  !insertmacro AddExtractMenus ".iso"
  !insertmacro AddExtractMenus ".udf"
  !insertmacro AddExtractMenus ".lzh"
  !insertmacro AddExtractMenus ".lha"
  !insertmacro AddExtractMenus ".zpaq"
  !insertmacro AddExtractMenus ".wim"
  !insertmacro AddExtractMenus ".isz"

  ; ── "Compress as ZIP" — headless one-click ZIP (all files) ──
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.CompressZip" "" "Compress as ZIP"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.CompressZip" "MUIVerb" "Compress as ZIP"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.CompressZip" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'

  ; ── "Compress as..." — jump to compress page (all files) ──
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.Compress" "" "Compress as..."
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.Compress" "MUIVerb" "Compress as..."
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.Compress" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX.Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'

  ; Same for directories
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip" "" "Compress as ZIP"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip" "MUIVerb" "Compress as ZIP"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'

  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.Compress" "" "Compress as..."
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.Compress" "MUIVerb" "Compress as..."
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.Compress" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX.Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'

  ; Write the sentinel so future upgrades skip registration
  WriteRegStr HKCU "Software\Classes\GeeZipX\ShellMenu" "Configured" "1"

  skip_all_menus:
!macroend

!macro preUnInstall
  ; ===================================================================
  ; Remove sentinel
  ; ===================================================================
  DeleteRegKey HKCU "Software\Classes\GeeZipX\ShellMenu"

  ; ===================================================================
  ; Remove extract menus — HKCU + legacy HKCR
  ; ===================================================================
  !insertmacro RemoveExtractMenus ".zip"
  !insertmacro RemoveExtractMenus ".zipx"
  !insertmacro RemoveExtractMenus ".tar"
  !insertmacro RemoveExtractMenus ".gz"
  !insertmacro RemoveExtractMenus ".bz2"
  !insertmacro RemoveExtractMenus ".br"
  !insertmacro RemoveExtractMenus ".lz4"
  !insertmacro RemoveExtractMenus ".zst"
  !insertmacro RemoveExtractMenus ".xz"
  !insertmacro RemoveExtractMenus ".lzma"
  !insertmacro RemoveExtractMenus ".lz"
  !insertmacro RemoveExtractMenus ".7z"
  !insertmacro RemoveExtractMenus ".rar"
  !insertmacro RemoveExtractMenus ".cab"
  !insertmacro RemoveExtractMenus ".asar"
  !insertmacro RemoveExtractMenus ".deb"
  !insertmacro RemoveExtractMenus ".cpio"
  !insertmacro RemoveExtractMenus ".iso"
  !insertmacro RemoveExtractMenus ".udf"
  !insertmacro RemoveExtractMenus ".lzh"
  !insertmacro RemoveExtractMenus ".lha"
  !insertmacro RemoveExtractMenus ".zpaq"
  !insertmacro RemoveExtractMenus ".wim"
  !insertmacro RemoveExtractMenus ".isz"

  ; ===================================================================
  ; Remove compress menus — HKCU + legacy HKCR
  ; ===================================================================

  ; ── HKCU (current location) ──
  DeleteRegKey HKCU "Software\Classes\*\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCU "Software\Classes\*\shell\GeeZipX.Compress"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GeeZipX.Compress"

  ; ── HKCR (legacy v0.7.4 and earlier) ──
  DeleteRegKey HKCR "*\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "*\shell\GeeZipX.Compress"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.Compress"
!macroend
