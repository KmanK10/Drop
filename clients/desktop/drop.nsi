; Per-user Drop installer. Unsigned. Does not start Drop at login.
Unicode true
RequestExecutionLevel user

!ifndef SOURCE_EXE
  !error "SOURCE_EXE is required"
!endif
!ifndef ICON
  !error "ICON is required"
!endif
!ifndef OUTFILE
  !error "OUTFILE is required"
!endif

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_WELCOMEPAGE_TITLE "Install Drop"
!define MUI_WELCOMEPAGE_TEXT "This installs Drop for the current Windows account and adds Drop to the Start menu.$\r$\n$\r$\nDrop does not start when you sign in. The server address and username stay in your AppData folder, the same place the portable program uses. Quit Drop if it is already open."
!define MUI_ABORTWARNING

!include "MUI2.nsh"
!include "LogicLib.nsh"

Name "Drop"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Drop"
InstallDirRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "InstallLocation"
ShowInstDetails show
ShowUninstDetails show
BrandingText "Drop"
SetCompressor /SOLID lzma

VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "Drop"
VIAddVersionKey "FileDescription" "Drop Setup"
VIAddVersionKey "FileVersion" "1.0.0"
VIAddVersionKey "ProductVersion" "1.0.0"
VIAddVersionKey "LegalCopyright" "MIT"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  nsExec::Exec 'cmd /c tasklist /NH /FI "IMAGENAME eq Drop.exe" | find /I "Drop.exe" >nul'
  Pop $0
  ${If} $0 == 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Quit Drop, then run this installer again."
    Abort
  ${EndIf}
FunctionEnd

Section "Drop"
  SetOutPath "$INSTDIR"
  File "/oname=Drop.exe" "${SOURCE_EXE}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Drop.lnk" "$INSTDIR\Drop.exe" "" "$INSTDIR\Drop.exe" 0

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "DisplayName" "Drop"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "DisplayIcon" "$INSTDIR\Drop.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "Publisher" "Drop"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "DisplayVersion" "1.0.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\Drop.lnk"
  Delete "$INSTDIR\Drop.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Drop"
SectionEnd
