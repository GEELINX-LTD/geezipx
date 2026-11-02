; Per-user installer for the independent egui frontend.
Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma
ManifestDPIAware true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "nsDialogs.nsh"

!ifndef REPO_ROOT
  !define REPO_ROOT "${__FILEDIR__}\..\..\.."
!endif
!ifndef APP_VERSION
  !error "Pass APP_VERSION from the geezipx-egui Cargo package version."
!endif
!ifndef APP_REVISION
  !define APP_REVISION "local"
!endif
!ifndef OUTPUT_DIR
  !define OUTPUT_DIR "${REPO_ROOT}\target\release\bundle\egui-nsis"
!endif

!define APP_NAME "GeeZipX"
!define APP_EXE "geezipx-egui.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\GeeZipXNative"
!define CERTIFICATES_KEY "${UNINSTALL_KEY}\InstallerCertificates"
!if /FileExists "${REPO_ROOT}\target\release\GeeZipXShell-test.cer"
  !ifndef TEST_CERT_THUMBPRINT
    !error "Pass TEST_CERT_THUMBPRINT for the bundled test certificate."
  !endif
  !define HAS_TEST_CERTIFICATE
!else
  !ifdef TEST_CERT_THUMBPRINT
    !error "TEST_CERT_THUMBPRINT requires GeeZipXShell-test.cer."
  !endif
!endif

Var CertificateChoice
Var CertificateCheckbox
Var CertificateAction
Var CertificateThumbprint
Var CertificatePath

Name "${APP_NAME}"
OutFile "${OUTPUT_DIR}\GeeZipX_${APP_VERSION}_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "GeeZipX"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${APP_NAME} installer (x64)"
VIAddVersionKey /LANG=1033 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "GeeZipX contributors"

!define MUI_ABORTWARNING
!define MUI_ICON "${REPO_ROOT}\crates\gui-egui\windows\icon.ico"
!define MUI_UNICON "${REPO_ROOT}\crates\gui-egui\windows\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
!define MUI_LANGDLL_REGISTRY_ROOT HKCU
!define MUI_LANGDLL_REGISTRY_KEY "${UNINSTALL_KEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "InstallerLanguage"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${REPO_ROOT}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!ifdef HAS_TEST_CERTIFICATE
Page custom CertificatePage CertificatePageLeave
!endif
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.CertificatePage un.CertificatePageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

LangString CertificateTitle ${LANG_SIMPCHINESE} "测试证书"
LangString CertificateTitle ${LANG_ENGLISH} "Test certificate"
LangString InstallCertificateSubtitle ${LANG_SIMPCHINESE} "选择是否信任此测试版本的签名证书。"
LangString InstallCertificateSubtitle ${LANG_ENGLISH} "Choose whether to trust the signing certificate for this test build."
LangString InstallCertificateText ${LANG_SIMPCHINESE} "此测试版的 Windows 11 一级右键扩展使用自签名证书，Windows 默认不信任。安装证书后，Windows 才能验证该扩展的签名并允许启用一级菜单。$\r$\n$\r$\n不安装证书仍可正常使用 GeeZipX 压缩、解压和浏览归档。$\r$\n$\r$\n勾选后会请求管理员权限，将随包公钥证书添加到本机“受信任人”（TrustedPeople）存储；卸载时可选择移除。"
LangString InstallCertificateText ${LANG_ENGLISH} "This test build's Windows 11 primary context menu uses a self-signed certificate, which Windows does not trust by default. Installing it lets Windows validate the extension's signature and enable that menu.$\r$\n$\r$\nWithout it, you can still compress, extract and browse archives.$\r$\n$\r$\nSelecting this option requests administrator permission to add the bundled public certificate to this computer's Trusted People store. You can choose to remove it during uninstall."
LangString InstallCertificateLabel ${LANG_SIMPCHINESE} "安装此测试版本的签名证书（需要管理员权限）"
LangString InstallCertificateLabel ${LANG_ENGLISH} "Install this build's test signing certificate (requires administrator permission)"
LangString RemoveCertificateSubtitle ${LANG_SIMPCHINESE} "选择是否移除此安装器添加的测试证书。"
LangString RemoveCertificateSubtitle ${LANG_ENGLISH} "Choose whether to remove test certificates added by this installer."
LangString RemoveCertificateText ${LANG_SIMPCHINESE} "只移除此安装器记录为自行添加的 GeeZipX 测试证书，包括升级前版本的证书。安装前已存在的证书保持不变。$\r$\n$\r$\n移除需要管理员权限，可能影响仍使用相同证书的其它 GeeZipX 测试版本。不勾选则保留证书。"
LangString RemoveCertificateText ${LANG_ENGLISH} "Only GeeZipX test certificates recorded as added by this installer will be removed, including certificates from earlier upgrades. Certificates present before installation are preserved.$\r$\n$\r$\nRemoval requires administrator permission and may affect other GeeZipX test copies using the same certificate. Leave unchecked to retain certificates."
LangString RemoveCertificateLabel ${LANG_SIMPCHINESE} "移除此安装器添加的测试证书（需要管理员权限）"
LangString RemoveCertificateLabel ${LANG_ENGLISH} "Remove test certificates added by this installer (requires administrator permission)"
LangString IntegrationRepairSkipped ${LANG_SIMPCHINESE} "GeeZipX 已安装，但系统快捷菜单未能恢复。若跳过了测试证书，请先信任随包证书，再到 GeeZipX 的“设置 → 系统集成”中重新应用菜单。压缩、解压和归档浏览仍可使用。"
LangString IntegrationRepairSkipped ${LANG_ENGLISH} "GeeZipX is installed, but its system shortcuts could not be restored. If you skipped the test certificate, trust the bundled certificate, then apply the menu again in GeeZipX Settings > System integration. Compression, extraction and archive browsing remain available."
LangString CertificateFailed ${LANG_SIMPCHINESE} "证书操作未完成或管理员授权被取消。应用文件和证书记录已保留，请重新运行安装器/卸载器重试，或不勾选证书选项。"
LangString CertificateFailed ${LANG_ENGLISH} "The certificate operation failed or administrator permission was cancelled. Application files and certificate records have been retained. Run setup/uninstall again to retry, or leave the certificate option unchecked."

; Installer and uninstaller each extract their own bundled helper. Never execute a
; script from the writable installation directory during an elevated operation.
!macro CertificateHelper PREFIX
Function ${PREFIX}RunCertificateAction
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=manage-test-certificate.ps1 "${__FILEDIR__}\manage-test-certificate.ps1"
  ClearErrors
  ExecWait '$\"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe$\" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $\"$PLUGINSDIR\manage-test-certificate.ps1$\" -Action $CertificateAction -Thumbprint $CertificateThumbprint -CertificatePath $\"$CertificatePath$\"' $0
  ${If} ${Errors}
    StrCpy $0 1
  ${EndIf}
FunctionEnd
!macroend
!ifdef HAS_TEST_CERTIFICATE
!insertmacro CertificateHelper ""
!endif
!insertmacro CertificateHelper "un."

!ifdef HAS_TEST_CERTIFICATE
Function CertificatePage
  !insertmacro MUI_HEADER_TEXT "$(CertificateTitle)" "$(InstallCertificateSubtitle)"
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 104u "$(InstallCertificateText)"
  Pop $0
  ${NSD_CreateCheckbox} 0 112u 100% 26u "$(InstallCertificateLabel)"
  Pop $CertificateCheckbox
  ${NSD_SetState} $CertificateCheckbox $CertificateChoice
  nsDialogs::Show
FunctionEnd

Function CertificatePageLeave
  ${NSD_GetState} $CertificateCheckbox $CertificateChoice
FunctionEnd
!endif

Function un.CertificatePage
  EnumRegValue $0 HKCU "${CERTIFICATES_KEY}" 0
  ${If} $0 == ""
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(CertificateTitle)" "$(RemoveCertificateSubtitle)"
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 90u "$(RemoveCertificateText)"
  Pop $0
  ${NSD_CreateCheckbox} 0 100u 100% 24u "$(RemoveCertificateLabel)"
  Pop $CertificateCheckbox
  ${NSD_SetState} $CertificateCheckbox $CertificateChoice
  nsDialogs::Show
FunctionEnd

Function un.CertificatePageLeave
  ${NSD_GetState} $CertificateCheckbox $CertificateChoice
FunctionEnd

Function .onInit
  SetRegView 64
  SetShellVarContext current
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "GeeZipX requires 64-bit Windows."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "GeeZipX requires Windows 10 or later."
    Abort
  ${EndIf}
  !insertmacro MUI_LANGDLL_DISPLAY
  StrCpy $CertificateChoice ${BST_UNCHECKED}
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/INSTALLCERT=" $1
  ${IfNot} ${Errors}
    ${If} $1 == "1"
      !ifdef HAS_TEST_CERTIFICATE
        StrCpy $CertificateChoice ${BST_CHECKED}
      !else
        SetErrorLevel 1
        Abort "This installer does not contain a test certificate."
      !endif
    ${ElseIf} $1 != "0"
      SetErrorLevel 1
      Abort "Expected /INSTALLCERT=0 or /INSTALLCERT=1."
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "GeeZipX" Main
  ; Deactivate the old surrogate before replacing a DLL that Windows may still host.
  ; Older, classic-menu-only versions have no DLL and do not know this maintenance flag.
  IfFileExists "$INSTDIR\geezipx_shell.dll" 0 shell_upgrade_ready
  ClearErrors
  ExecWait '$\"$INSTDIR\${APP_EXE}$\" --prepare-shell-upgrade' $0
  ${If} ${Errors}
  ${OrIf} $0 != 0
    DetailPrint "Could not deactivate the Windows 11 menu for upgrade. Close its menu and retry."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ClearErrors
  Delete "$INSTDIR\geezipx_shell.dll"
  ${If} ${Errors}
    DetailPrint "The old Explorer extension is still in use. Close its menu and retry."
    SetErrorLevel 1
    Abort
  ${EndIf}
  shell_upgrade_ready:
  SetOutPath "$INSTDIR"
  File "${REPO_ROOT}\target\release\${APP_EXE}"
  File "${REPO_ROOT}\target\release\geezipx_shell.dll"
  File "${REPO_ROOT}\target\release\GeeZipXShell.msix"
  File "${REPO_ROOT}\LICENSE"
  SetOutPath "$INSTDIR\licenses"
  ; Remove the previous Droid font license when upgrading an existing installation.
  Delete "$INSTDIR\licenses\FONT-APACHE-2.0.txt"
  File "${REPO_ROOT}\crates\gui-egui\assets\*.txt"
  SetOutPath "$INSTDIR"
  FileOpen $0 "$INSTDIR\build-info.txt" w
  FileWrite $0 "GeeZipX ${APP_VERSION}$\r$\nCommit: ${APP_REVISION}$\r$\n"
  FileClose $0
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Migrate shortcuts created by the previous display name.
  Delete "$SMPROGRAMS\GeeZipX Native\GeeZipX Native.lnk"
  Delete "$SMPROGRAMS\GeeZipX Native\Uninstall.lnk"
  RMDir "$SMPROGRAMS\GeeZipX Native"
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "GeeZipX contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/GEELINX-LTD/geezipx"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "BuildRevision" "${APP_REVISION}"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0

  !ifdef HAS_TEST_CERTIFICATE
    ${If} $CertificateChoice == ${BST_CHECKED}
      InitPluginsDir
      SetOutPath "$PLUGINSDIR"
      File /oname=GeeZipXShell-test.cer "${REPO_ROOT}\target\release\GeeZipXShell-test.cer"
      StrCpy $CertificateAction "Install"
      StrCpy $CertificateThumbprint "${TEST_CERT_THUMBPRINT}"
      StrCpy $CertificatePath "$PLUGINSDIR\GeeZipXShell-test.cer"
      Call RunCertificateAction
      ${If} $0 == 0
        WriteRegDWORD HKCU "${CERTIFICATES_KEY}" "$CertificateThumbprint" 1
      ${ElseIf} $0 != 10
        DetailPrint "$(CertificateFailed)"
        MessageBox MB_OK|MB_ICONSTOP "$(CertificateFailed)" /SD IDOK
        SetErrorLevel 1
        Abort
      ${EndIf}
      ; Exit 10 means trust already existed. Keep any earlier ownership record,
      ; but do not claim a certificate imported by the user or another installer.
      SetOutPath "$INSTDIR"
    ${EndIf}
  !endif

  ; Preserve the user's chosen associations/menu when the install path changes.
  ClearErrors
  ExecWait '$\"$INSTDIR\${APP_EXE}$\" --repair-shell-integration' $0
  ${If} ${Errors}
  ${OrIf} $0 != 0
    !ifdef HAS_TEST_CERTIFICATE
      ${If} $CertificateChoice == ${BST_UNCHECKED}
        ; A declined test certificate may prevent the new MSIX identity from
        ; registering. Keep the archive app usable and explain how to retry.
        DetailPrint "$(IntegrationRepairSkipped)"
        MessageBox MB_OK|MB_ICONINFORMATION "$(IntegrationRepairSkipped)" /SD IDOK
        Goto shell_repair_done
      ${EndIf}
    !endif
    DetailPrint "Could not repair GeeZipX system integration. Retry from Settings."
    SetErrorLevel 1
    Abort
  ${EndIf}
  !ifdef HAS_TEST_CERTIFICATE
    shell_repair_done:
  !endif
SectionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext current
  !insertmacro MUI_UNGETLANGUAGE
  StrCpy $CertificateChoice ${BST_UNCHECKED}
  ${un.GetParameters} $0
  ClearErrors
  ${un.GetOptions} $0 "/REMOVECERT=" $1
  ${IfNot} ${Errors}
    ${If} $1 == "1"
      StrCpy $CertificateChoice ${BST_CHECKED}
    ${ElseIf} $1 != "0"
      SetErrorLevel 1
      Abort "Expected /REMOVECERT=0 or /REMOVECERT=1."
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ; The installed binary owns the extension/COM list; clean it before deleting it.
  IfFileExists "$INSTDIR\${APP_EXE}" 0 integration_removed
  ClearErrors
  ExecWait '$\"$INSTDIR\${APP_EXE}$\" --remove-shell-integration' $0
  ${If} ${Errors}
  ${OrIf} $0 != 0
    DetailPrint "Could not remove GeeZipX system integration. Application files retained."
    SetErrorLevel 1
    Abort
  ${EndIf}
  integration_removed:
  IfFileExists "$INSTDIR\geezipx_shell.dll" 0 shell_dll_removed
  ClearErrors
  Delete "$INSTDIR\geezipx_shell.dll"
  ${If} ${Errors}
    DetailPrint "The Explorer extension is still in use. Application files retained; close its menu and retry."
    SetErrorLevel 1
    Abort
  ${EndIf}
  shell_dll_removed:
  ${If} $CertificateChoice == ${BST_CHECKED}
    StrCpy $CertificateAction "Remove"
    StrCpy $CertificatePath ""
    certificate_remove_next:
    EnumRegValue $CertificateThumbprint HKCU "${CERTIFICATES_KEY}" 0
    ${If} $CertificateThumbprint != ""
      Call un.RunCertificateAction
      ${If} $0 != 0
        DetailPrint "$(CertificateFailed)"
        MessageBox MB_OK|MB_ICONSTOP "$(CertificateFailed)" /SD IDOK
        SetErrorLevel 1
        Abort
      ${EndIf}
      DeleteRegValue HKCU "${CERTIFICATES_KEY}" "$CertificateThumbprint"
      Goto certificate_remove_next
    ${EndIf}
    SetOutPath "$INSTDIR"
  ${EndIf}
  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$SMPROGRAMS\GeeZipX Native\GeeZipX Native.lnk"
  Delete "$SMPROGRAMS\GeeZipX Native\Uninstall.lnk"
  RMDir "$SMPROGRAMS\GeeZipX Native"
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\GeeZipXShell.msix"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\build-info.txt"
  Delete "$INSTDIR\licenses\EGUI-FONTS-LICENSES.txt"
  Delete "$INSTDIR\licenses\FONT-APACHE-2.0.txt"
  Delete "$INSTDIR\licenses\FONT-LICENSE.txt"
  Delete "$INSTDIR\licenses\FONT-NOTICE.txt"
  RMDir "$INSTDIR\licenses"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
