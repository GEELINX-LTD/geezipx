; GeeZipX NSIS installer hooks
; =====================================================================
; Adds Windows Shell context menu (right-click) verbs:
;
;   Archive files (.zip / .7z / .rar / ...):
;     ┌─────────────────────────┐
;     │ 解压缩到当前文件夹        │  → /extract-here "%1"
;     │ 解压缩到...              │  → /extract "%1"
;     │ 用 GeeZipX 打开          │  → /open "%1"
;     └─────────────────────────┘
;
;   All files (*) and directories:
;     ┌──────────────────┐
;     │ 压缩为 ZIP        │  → /compress-zip "%1"
;     │ 压缩为...          │  → /compress "%1"
;     └──────────────────┘
;
; Registered via static registry verbs under HKCR. On Windows 11 these
; appear in the "Show more options" (classic) submenu.
; =====================================================================

!macro AddExtractMenus ext
  ; ── "解压缩到当前文件夹" (top) ──
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "" ""
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "MUIVerb" "解压缩到当前文件夹"
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere\command" "" '"$INSTDIR\geezipx-gui.exe" /extract-here "%1"'

  ; ── "解压缩到..." (middle) ──
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "" ""
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "MUIVerb" "解压缩到..."
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract\command" "" '"$INSTDIR\geezipx-gui.exe" /extract "%1"'

  ; ── "用 GeeZipX 打开" (bottom) ──
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open" "" ""
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open" "MUIVerb" "用 GeeZipX 打开"
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open\command" "" '"$INSTDIR\geezipx-gui.exe" /open "%1"'
!macroend

!macro RemoveExtractMenus ext
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.ExtractHere"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Extract"
  DeleteRegKey HKCR "SystemFileAssociations\${ext}\shell\GeeZipX.Open"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell\GeeZipX"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}\shell"
  DeleteRegKey /ifempty HKCR "SystemFileAssociations\${ext}"
!macroend

!macro preInstall
  ; ===================================================================
  ; Extract / Open menus — one per supported archive extension
  ; ===================================================================
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

  ; ===================================================================
  ; Compress menus — for all files (*) and directories
  ; ===================================================================

  ; ── "压缩为 ZIP" (top) — headless one-click ZIP ──
  ; MultiSelectModel omitted intentionally: default (Document) mode passes
  ; all selected files as arguments in a single invocation, so one click
  ; produces one ZIP containing every selected item.
  WriteRegStr HKCR "*\shell\GeeZipX.CompressZip" "" ""
  WriteRegStr HKCR "*\shell\GeeZipX.CompressZip" "MUIVerb" "压缩为 ZIP"
  WriteRegStr HKCR "*\shell\GeeZipX.CompressZip" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "*\shell\GeeZipX.CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'

  ; ── "压缩为..." (bottom) — jump to compress page ──
  WriteRegStr HKCR "*\shell\GeeZipX.Compress" "" ""
  WriteRegStr HKCR "*\shell\GeeZipX.Compress" "MUIVerb" "压缩为..."
  WriteRegStr HKCR "*\shell\GeeZipX.Compress" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "*\shell\GeeZipX.Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCR "*\shell\GeeZipX.Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'

  ; Same for directories (no MultiSelectModel — Document default).
  WriteRegStr HKCR "Directory\shell\GeeZipX.CompressZip" "" ""
  WriteRegStr HKCR "Directory\shell\GeeZipX.CompressZip" "MUIVerb" "压缩为 ZIP"
  WriteRegStr HKCR "Directory\shell\GeeZipX.CompressZip" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "Directory\shell\GeeZipX.CompressZip\command" "" '"$INSTDIR\geezipx-gui.exe" /compress-zip "%1"'

  WriteRegStr HKCR "Directory\shell\GeeZipX.Compress" "" ""
  WriteRegStr HKCR "Directory\shell\GeeZipX.Compress" "MUIVerb" "压缩为..."
  WriteRegStr HKCR "Directory\shell\GeeZipX.Compress" "Icon" "$INSTDIR\geezipx-gui.exe,0"
  WriteRegStr HKCR "Directory\shell\GeeZipX.Compress" "MultiSelectModel" "Player"
  WriteRegStr HKCR "Directory\shell\GeeZipX.Compress\command" "" '"$INSTDIR\geezipx-gui.exe" /compress "%1"'
!macroend

!macro preUnInstall
  ; ===================================================================
  ; Remove extract / open menus
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

  ; ===================================================================
  ; Remove compress menus
  ; ===================================================================
  DeleteRegKey HKCR "*\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "*\shell\GeeZipX.Compress"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.CompressZip"
  DeleteRegKey HKCR "Directory\shell\GeeZipX.Compress"
!macroend
