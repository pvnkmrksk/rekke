Unicode true
!include "MUI2.nsh"
Name "Rekke"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Rekke"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ICON "${PACKAGE}\rekke.ico"
!define MUI_UNICON "${PACKAGE}\rekke.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\rekke.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Section "Rekke"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${PACKAGE}\rekke.exe"
  File "${PACKAGE}\rekke.ico"
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
  CreateShortcut "$SMPROGRAMS\Rekke.lnk" "$INSTDIR\rekke.exe" "" "$INSTDIR\rekke.exe" 0
  CreateShortcut "$DESKTOP\Rekke.lnk" "$INSTDIR\rekke.exe" "" "$INSTDIR\rekke.exe" 0
  CreateShortcut "$SMPROGRAMS\Rekke - start all rigs.lnk" "$INSTDIR\rekke.exe" "--rigs" "$INSTDIR\rekke.exe" 0
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "DisplayName" "Rekke"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "Publisher" "pvnkmrksk"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "DisplayIcon" "$INSTDIR\rekke.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "URLInfoAbout" "https://github.com/pvnkmrksk/rekke"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\Rekke.lnk"
  Delete "$DESKTOP\Rekke.lnk"
  Delete "$SMPROGRAMS\Rekke - start all rigs.lnk"
  Delete "$INSTDIR\rekke.exe"
  Delete "$INSTDIR\rekke.ico"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\INSTALL.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  RMDir /r "$INSTDIR\defaults"
  RMDir /r "$INSTDIR\examples"
  RMDir /r "$INSTDIR\samples"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rekke"
  ; User configs/backgrounds/recordings in APPDATA\Rekke are preserved.
SectionEnd
