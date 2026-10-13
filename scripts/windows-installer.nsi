Unicode true
!include "MUI2.nsh"
Name "Rekkefly"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Rekkefly"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ICON "${PACKAGE}\rekkefly.ico"
!define MUI_UNICON "${PACKAGE}\rekkefly.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\rekkefly.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Section "Rekkefly"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${PACKAGE}\rekkefly.exe"
  File "${PACKAGE}\rekkefly.ico"
  File "${PACKAGE}\README.md"
  File "${PACKAGE}\INSTALL.txt"
  File "${PACKAGE}\LICENSE"
  File "${PACKAGE}\THIRD_PARTY_NOTICES.txt"
  SetOutPath "$INSTDIR\defaults"
  File /r "${PACKAGE}\defaults\*.*"
  SetOutPath "$INSTDIR\examples"
  File /r "${PACKAGE}\examples\*.*"
  SetOutPath "$INSTDIR\samples"
  File /r "${PACKAGE}\samples\*.*"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Rekkefly.lnk" "$INSTDIR\rekkefly.exe" "" "$INSTDIR\rekkefly.exe" 0
  CreateShortcut "$DESKTOP\Rekkefly.lnk" "$INSTDIR\rekkefly.exe" "" "$INSTDIR\rekkefly.exe" 0
  CreateShortcut "$SMPROGRAMS\Rekkefly - start all rigs.lnk" "$INSTDIR\rekkefly.exe" "--rigs" "$INSTDIR\rekkefly.exe" 0
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "DisplayName" "Rekkefly"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "Publisher" "pvnkmrksk"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "DisplayIcon" "$INSTDIR\rekkefly.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "URLInfoAbout" "https://pvnkmrksk.github.io/rekkefly/"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\Rekkefly.lnk"
  Delete "$DESKTOP\Rekkefly.lnk"
  Delete "$SMPROGRAMS\Rekkefly - start all rigs.lnk"
  Delete "$INSTDIR\rekkefly.exe"
  Delete "$INSTDIR\rekkefly.ico"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\INSTALL.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  RMDir /r "$INSTDIR\defaults"
  RMDir /r "$INSTDIR\examples"
  RMDir /r "$INSTDIR\samples"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekkefly"
  ; User configs/backgrounds/recordings in APPDATA\Rekkefly are preserved.
SectionEnd
