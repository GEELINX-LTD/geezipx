; GeeZipX NSIS installer hooks
; =====================================================================
; Strategy (v0.7.6+)
; ------------------
; Registry keys are written to HKCU\Software\Classes (per-user, no
; admin required). This matches the runtime `set_shell_menu` Tauri
; command, which also writes to HKCU, so the Settings page can
; dynamically toggle verbs without fighting the installer.
;
; Sub-menu structure (v0.7.6+)
; -----------------------------
; Verbs are grouped under a parent `GeeZipX` key using nested `shell`
; sub-keys and an `ExtendedSubCommandsKey` (self-referencing, HKCR-relative)
; so Explorer discovers the children and renders a cascaded fly-out menu:
;
;   Archive extensions:
;     HKCU\...\shell\GeeZipX                        ← parent (MUIVerb, Icon, ExtendedSubCommandsKey)
;     HKCU\...\shell\GeeZipX\shell\Extract           ← child  (MUIVerb, command)
;     HKCU\...\shell\GeeZipX\shell\ExtractHere
;
;   * (all files) / Directory:
;     HKCU\...\shell\GeeZipX                        ← parent
;     HKCU\...\shell\GeeZipX\shell\Compress          ← child
;     HKCU\...\shell\GeeZipX\shell\CompressZip
;
; i18n
; ----
; NSIS variable $LANGUAGE is set by the installer UI language selection
; (1033 = English, 2052 = SimpChinese).  Labels are written in the
; matching language so the menu matches the installer language on first
; install.  The runtime Setings page can override later.
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
;   3. HKCU\Software\Classes parent keys (current location)
;   4. HKCR verb keys (legacy v0.7.4 and earlier)
;
; On Windows 11 these entries appear under "Show more options" (classic
; menu) — this is a system limitation.
; =====================================================================

; ---------------------------------------------------------------------
; i18n helper: resolve localised labels from NSIS $LANGUAGE
; ---------------------------------------------------------------------

Var _geezip_lang

!macro GeeZipXDetectLocale
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy $_geezip_lang "zh"
  ${Else}
    StrCpy $_geezip_lang "en"
  ${EndIf}
!macroend

!macro GeeZipXExtractLabel lang_out
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy ${lang_out} "解压缩到..."
  ${Else}
    StrCpy ${lang_out} "Extract to..."
  ${EndIf}
!macroend

!macro GeeZipXExtractHereLabel lang_out
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy ${lang_out} "解压缩到当前文件夹"
  ${Else}
    StrCpy ${lang_out} "Extract here"
  ${EndIf}
!macroend

!macro GeeZipXCompressLabel lang_out
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy ${lang_out} "压缩为..."
  ${Else}
    StrCpy ${lang_out} "Compress as..."
  ${EndIf}
!macroend

!macro GeeZipXCompressZipLabel lang_out
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy ${lang_out} "压缩为 ZIP"
  ${Else}
    StrCpy ${lang_out} "Compress as ZIP"
  ${EndIf}
!macroend

; ---------------------------------------------------------------------
; Extract menus (per extension) — parent + two children
; ---------------------------------------------------------------------

!macro AddExtractMenus ext
  ; parent key (MUIVerb + Icon + ExtendedSubCommandsKey)
  ; ExtendedSubCommandsKey (self-referencing, HKCR-relative) is required for
  ; Explorer to expand the nested shell children into a cascading menu.
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "MUIVerb" "GeeZipX"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "ExtendedSubCommandsKey" "SystemFileAssociations\${ext}\shell\GeeZipX"

  ; child: "Extract to..." (nested under parent)
  !insertmacro GeeZipXExtractLabel $0
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX\shell\Extract" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX\shell\Extract\command" "" '"$INSTDIR\geezipx-gui.exe" /extract "%1"'

  ; child: "Extract here" (nested under parent)
  !insertmacro GeeZipXExtractHereLabel $0
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX\shell\ExtractHere" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX\shell\ExtractHere\command" "" '"$INSTDIR\geezipx-gui.exe" /extract-here "%1"'
!macroend

!macro RemoveExtractMenus ext
  ; recursively delete the GeeZipX parent tree (also removes nested children)
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX"
  ; legacy: clean old flat sibling keys from pre-nested versions
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${ext}\shell"
  DeleteRegKey /ifempty HKCU "Software\Classes\SystemFileAssociations\${ext}"
  ; HKCR legacy
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell\GeeZipX"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}"
!macroend

; ---------------------------------------------------------------------
; Compress menus (* and Directory) — parent + two children
; ---------------------------------------------------------------------

!macro AddCompressMenus
  ; Parent keys (MUIVerb + Icon + ExtendedSubCommandsKey)
  ; ExtendedSubCommandsKey (self-referencing, HKCR-relative) is required for
  ; Explorer to expand the nested shell children into a cascading menu.
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX" "MUIVerb" "GeeZipX"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX" "ExtendedSubCommandsKey" "*\shell\GeeZipX"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX" "MUIVerb" "GeeZipX"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX" "ExtendedSubCommandsKey" "Directory\shell\GeeZipX"

  ; Child: "Compress as..." (nested under parent)
  !insertmacro GeeZipXCompressLabel $0
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX\shell\Compress" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX\shell\Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX\shell\Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX\shell\Compress" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX\shell\Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX\shell\Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'

  ; Child: "Compress as ZIP" (nested under parent)
  !insertmacro GeeZipXCompressZipLabel $0
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX\shell\CompressZip" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\*\shell\GeeZipX\shell\CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX\shell\CompressZip" "MUIVerb" "$0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GeeZipX\shell\CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'
!macroend

!macro RemoveCompressMenus
  ; recursively delete parent trees (also removes nested children)
  DeleteRegKey HKCU "Software\Classes\*\shell\GeeZipX"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GeeZipX"
  ; legacy: clean old flat sibling keys
  DeleteRegKey HKCU "Software\Classes\*\shell\GeeZipX.Compress"
  DeleteRegKey HKCU "Software\Classes\*\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GeeZipX.Compress"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GeeZipX.CompressZip"
  ; HKCR legacy
  DeleteRegKey HKCR "*\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "*\shell\GeeZipX.Compress"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.Compress"
!macroend

; ---------------------------------------------------------------------
; Fix parent keys — repair intermediate-build state without recreating
; verbs the user may have turned off.  Only operates on keys that
; already exist (detected via MUIVerb, a stable named value).
; Also cleans old flat sibling keys so they don't coexist with the
; nested shell structure.
; ---------------------------------------------------------------------

!macro FixExtractParentKey ext
  ; Check if parent key exists by reading MUIVerb — a stable named
  ; value written by every version of the nested shell installer.
  ; ClearErrors + ReadRegStr + ${IfNot} ${Errors} is the recommended
  ; NSIS pattern to distinguish "key exists" from "key does not exist".
  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "MUIVerb"
  ${IfNot} ${Errors}
    ; Parent key exists — write the ExtendedSubCommandsKey required
    ; for Explorer to render the cascaded fly-out, and remove any old
    ; SubCommands value left from pre-nested or intermediate builds.
    WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "ExtendedSubCommandsKey" "SystemFileAssociations\${ext}\shell\GeeZipX"
    DeleteRegValue HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX" "SubCommands"
  ${EndIf}
  ; Clean old flat sibling keys (idempotent — DeleteRegKey on missing
  ; targets is a no-op in NSIS).
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  ; HKCR legacy
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
!macroend

!macro FixExtractParentKeys
  !insertmacro FixExtractParentKey ".zip"
  !insertmacro FixExtractParentKey ".zipx"
  !insertmacro FixExtractParentKey ".tar"
  !insertmacro FixExtractParentKey ".gz"
  !insertmacro FixExtractParentKey ".bz2"
  !insertmacro FixExtractParentKey ".br"
  !insertmacro FixExtractParentKey ".lz4"
  !insertmacro FixExtractParentKey ".zst"
  !insertmacro FixExtractParentKey ".xz"
  !insertmacro FixExtractParentKey ".lzma"
  !insertmacro FixExtractParentKey ".lz"
  !insertmacro FixExtractParentKey ".7z"
  !insertmacro FixExtractParentKey ".rar"
  !insertmacro FixExtractParentKey ".cab"
  !insertmacro FixExtractParentKey ".asar"
  !insertmacro FixExtractParentKey ".deb"
  !insertmacro FixExtractParentKey ".cpio"
  !insertmacro FixExtractParentKey ".iso"
  !insertmacro FixExtractParentKey ".udf"
  !insertmacro FixExtractParentKey ".lzh"
  !insertmacro FixExtractParentKey ".lha"
  !insertmacro FixExtractParentKey ".zpaq"
  !insertmacro FixExtractParentKey ".wim"
  !insertmacro FixExtractParentKey ".isz"
!macroend

!macro FixCompressParentKey class
  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\${class}\shell\GeeZipX" "MUIVerb"
  ${IfNot} ${Errors}
    WriteRegStr HKCU "Software\Classes\${class}\shell\GeeZipX" "ExtendedSubCommandsKey" "${class}\shell\GeeZipX"
    DeleteRegValue HKCU "Software\Classes\${class}\shell\GeeZipX" "SubCommands"
  ${EndIf}
  DeleteRegKey HKCU "Software\Classes\${class}\shell\GeeZipX.Compress"
  DeleteRegKey HKCU "Software\Classes\${class}\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "${class}\shell\GeeZipX.Compress"
  DeleteRegKey HKCR "${class}\shell\GeeZipX.CompressZip"
!macroend

!macro FixCompressParentKeys
  !insertmacro FixCompressParentKey "*"
  !insertmacro FixCompressParentKey "Directory"
!macroend

; ---------------------------------------------------------------------
; preInstall — smart registration on first install or upgrade
; ---------------------------------------------------------------------

!macro preInstall
  ; ===================================================================
  ; Step 1 — Check sentinel. If the runtime has ever written the
  ; sentinel, the user has made a deliberate choice.  Fix any existing
  ; parent keys that are missing ExtendedSubCommandsKey (e.g. from the
  ; intermediate v0.7.6 build that had nested shell but no cascading
  ; declaration), clean old flat siblings, and exit.  Do NOT recreate
  ; verbs the user may have manually turned off.
  ; ===================================================================
  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\GeeZipX\ShellMenu" "Configured"
  ${IfNot} ${Errors}
  ${AndIf} $0 == "1"
    !insertmacro FixExtractParentKeys
    !insertmacro FixCompressParentKeys
    Goto skip_all_menus
  ${EndIf}

  ; ===================================================================
  ; Step 2 — No sentinel.  Clean any old flat sibling or lingering
  ; nested structures, then register the current defaults.
  ; ===================================================================
  !insertmacro FixExtractParentKeys
  !insertmacro FixCompressParentKeys

  ; ===================================================================
  ; Step 3 — Fresh install. Register all four verbs (parent + children)
  ; and write the sentinel.
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

  ; Compress menus — * + Directory (parent + children, with i18n labels)
  !insertmacro AddCompressMenus

  ; Write the sentinel so future upgrades skip registration
  WriteRegStr HKCU "Software\Classes\GeeZipX\ShellMenu" "Configured" "1"

  skip_all_menus:
!macroend

; ---------------------------------------------------------------------
; preUnInstall — thorough cleanup
; ---------------------------------------------------------------------

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
  !insertmacro RemoveCompressMenus
!macroend
