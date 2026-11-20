; NSIS installer for AutoPara (Windows).
;
; Per-user: installs to %LOCALAPPDATA%\Programs\AutoPara, no admin rights, no UAC prompt.
; Same uninstall key and folder as the old Python installer, so it upgrades those installs in place.
; The in-app updater runs this with /S.
;
; Build:  makensis /DAPP_VERSION=2.0.4 installer\AutoPara.nsi
; Needs target\release\autopara.exe (cargo build --release -p autopara).
;
; UTF-8 with BOM on purpose: without the BOM makensis mangles the Ukrainian strings.

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef APP_VERSION
  !define APP_VERSION "0.0.0"
!endif
!define APP_NAME      "AutoPara"
; Window titles the app uses (uk / en); used to tell whether it is running.
!define TITLE_UK      "AutoPara — автозапуск созвонів і пар"
!define TITLE_EN      "AutoPara — auto-join calls & classes"
!define APP_PUBLISHER "AutoPara"
!define APP_EXE       "AutoPara.exe"
!define UNINST_KEY    "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"
!define RUN_KEY       "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${APP_NAME}"
OutFile "..\dist\AutoPara-${APP_VERSION}-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "Software\${APP_NAME}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON   "..\crates\autopara-app\icons\icon.ico"
!define MUI_UNICON "..\crates\autopara-app\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Запустити AutoPara"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Ukrainian"

VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "ProductName"     "${APP_NAME}"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "FileDescription" "${APP_NAME} Setup"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "FileVersion"     "${APP_VERSION}"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey /LANG=${LANG_UKRAINIAN} "LegalCopyright"  "${APP_PUBLISHER}"

; The exe is locked while the app runs (often hidden in the tray; closing the window only
; hides it), so stop it before copying or deleting.
!macro STOP_APP
  nsExec::Exec 'taskkill /F /IM "${APP_EXE}"'
  Pop $0
  Sleep 700
!macroend

; ---------------------------------------------------------------- install

Section "AutoPara (обов'язково)" SEC_CORE
  SectionIn RO
  SetOutPath "$INSTDIR"

  !insertmacro STOP_APP
  ; leftovers of the old Python build
  RMDir /r "$INSTDIR\_internal"

  File "/oname=${APP_EXE}" "..\target\release\autopara.exe"

  WriteRegStr HKCU "Software\${APP_NAME}" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr   HKCU "${UNINST_KEY}" "DisplayName"     "${APP_NAME}"
  WriteRegStr   HKCU "${UNINST_KEY}" "DisplayVersion"  "${APP_VERSION}"
  WriteRegStr   HKCU "${UNINST_KEY}" "Publisher"       "${APP_PUBLISHER}"
  WriteRegStr   HKCU "${UNINST_KEY}" "DisplayIcon"     "$INSTDIR\${APP_EXE}"
  WriteRegStr   HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr   HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr   HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Видалити ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Ярлик на робочому столі" SEC_DESKTOP
  ; a silent run is an in-app update: don't bring back a shortcut the user deleted
  IfSilent +2
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
SectionEnd

Section "Запускати разом із Windows" SEC_AUTOSTART
  ; Same value as autostart::startup_command(), so the app sees it as already on.
  ; On an update the app re-syncs this with its own setting at the next start.
  WriteRegStr HKCU "${RUN_KEY}" "${APP_NAME}" '"$INSTDIR\${APP_EXE}" --hidden'
SectionEnd

LangString DESC_CORE      ${LANG_UKRAINIAN} "Програма AutoPara."
LangString DESC_DESKTOP   ${LANG_UKRAINIAN} "Створити ярлик на робочому столі."
LangString DESC_AUTOSTART ${LANG_UKRAINIAN} "Запускати AutoPara у трей разом із Windows, щоб пари \
відкривалися навіть тоді, коли ви забули увімкнути програму. Це можна змінити в налаштуваннях."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_CORE}      $(DESC_CORE)
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_DESKTOP}   $(DESC_DESKTOP)
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_AUTOSTART} $(DESC_AUTOSTART)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function .onInit
  IfSilent done
  FindWindow $0 "" "${TITLE_UK}"
  ${If} $0 = 0
    FindWindow $0 "" "${TITLE_EN}"
  ${EndIf}
  ${If} $0 <> 0
    MessageBox MB_OKCANCEL|MB_ICONINFORMATION \
      "AutoPara зараз працює. Натисніть OK, щоб закрити її й продовжити." \
      IDOK done
    Abort
  ${EndIf}
  done:
FunctionEnd

; ---------------------------------------------------------------- uninstall

Section "Uninstall"
  !insertmacro STOP_APP
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\_internal"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Видалити ${APP_NAME}.lnk"
  RMDir  "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegValue HKCU "${RUN_KEY}" "${APP_NAME}"
  DeleteRegKey   HKCU "${UNINST_KEY}"
  DeleteRegKey   HKCU "Software\${APP_NAME}"

  ; Schedule and settings live in %APPDATA%\AutoPara. Kept by default (also on /S), so a
  ; reinstall doesn't lose them; only deleted when the user explicitly says yes.
  IfSilent keep_data
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
    "Видалити також збережений розклад і налаштування?$\r$\n$\r$\n$APPDATA\${APP_NAME}" \
    IDNO keep_data
  RMDir /r "$APPDATA\${APP_NAME}"
  keep_data:
SectionEnd
