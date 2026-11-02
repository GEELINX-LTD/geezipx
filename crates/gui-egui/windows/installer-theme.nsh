; Shared with the light palette in src/theme.rs. Keep artwork derived from icon.png.
!define GX_CANVAS "F8F9FC"
!define GX_SURFACE "FFFFFF"
!define GX_TEXT "262D40"
!define GX_MUTED "758096"
!define GX_ACCENT "5263E0"
!define GX_TINT "EAEDFF"
!define MUI_BGCOLOR "${GX_CANVAS}"
!define MUI_TEXTCOLOR "${GX_TEXT}"
!define MUI_INSTFILESPAGE_COLORS "${GX_TEXT} ${GX_SURFACE}"
!define MUI_FINISHPAGE_LINK_COLOR "${GX_ACCENT}"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "${__FILEDIR__}\installer-header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${__FILEDIR__}\installer-sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "${__FILEDIR__}\installer-sidebar.bmp"
!define MUI_CUSTOMFUNCTION_GUIINIT ApplyTheme
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.ApplyTheme

!macro ThemeFunctions PREFIX
Function ${PREFIX}ApplyTheme
  SetCtlColors $HWNDPARENT ${GX_TEXT} ${GX_CANVAS}
  SetCtlColors $mui.Branding.Text ${GX_MUTED} ${GX_CANVAS}
  SetCtlColors $mui.Branding.Background ${GX_TEXT} ${GX_CANVAS}
  SetCtlColors $mui.Header.Text ${GX_TEXT} ${GX_SURFACE}
  SetCtlColors $mui.Header.SubText ${GX_MUTED} ${GX_SURFACE}
  SetCtlColors $mui.Header.Background ${GX_TEXT} ${GX_SURFACE}
  SetCtlColors $mui.Header.Image ${GX_TEXT} ${GX_SURFACE}
FunctionEnd

; Standard native controls preserve keyboard navigation, focus and DPI scaling.
Function ${PREFIX}ThemePage
  FindWindow $0 "#32770" "" $HWNDPARENT
  SetCtlColors $0 ${GX_TEXT} ${GX_CANVAS}
  GetDlgItem $1 $0 1006
  SetCtlColors $1 ${GX_TEXT} ${GX_CANVAS}
  GetDlgItem $1 $0 1040
  SetCtlColors $1 ${GX_MUTED} ${GX_CANVAS}
  GetDlgItem $1 $0 1020
  SetCtlColors $1 ${GX_TEXT} ${GX_CANVAS}
  GetDlgItem $1 $0 1023
  SetCtlColors $1 ${GX_MUTED} ${GX_CANVAS}
  GetDlgItem $1 $0 1024
  SetCtlColors $1 ${GX_MUTED} ${GX_CANVAS}
FunctionEnd
!macroend

!macro ThemeLabel TEXT X Y WIDTH HEIGHT
  ${NSD_CreateLabel} ${X} ${Y} ${WIDTH} ${HEIGHT} "${TEXT}"
  Pop $0
  SetCtlColors $0 ${GX_TEXT} ${GX_CANVAS}
!macroend

!macro ThemeCheckbox TEXT Y VAR
  ${NSD_CreateCheckbox} 0 ${Y} 100% 26u "${TEXT}"
  Pop ${VAR}
  SetCtlColors ${VAR} ${GX_TEXT} ${GX_CANVAS}
!macroend
