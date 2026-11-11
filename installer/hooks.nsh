; Leeep Connect — hooks for the Tauri NSIS installer (bundle.windows.nsis.installerHooks).
;
; Tauri's installer already copies the app (per-machine, needs admin) and the
; bundled resources, adds Start-menu/uninstall entries, and offers to launch the
; app. These hooks add what makes setup zero-touch for a school:
;   1. install + enrol the remote-control engine agent as a Windows service
;   2. register the leeep-connect:// link for every user (HKLM)
;   3. start the app in the tray at every login
; and undo all of it on uninstall.
;
; The agent file (resources: meshagent/meshagent.exe) is the "Leeep Clients"
; group agent downloaded from the engine (Add Agent → Windows x64 → Background &
; interactive). Its group/server settings are embedded, so it needs no config.
;
; VERIFY (spike): meshagent switches -fullinstall / -fulluninstall and the
; service name "LeeepConnect" (from agentCustomization.serviceName).
; VERIFY (first build): MAINBINARYNAME and $UpdateMode come from Tauri's NSIS
; template; check the generated installer.nsi under target/release/nsis.

!define LC_PROTOCOL "leeep-connect"
!define LC_SERVICE "LeeepConnect"
!define LC_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

!macro NSIS_HOOK_POSTINSTALL
  ; 1. Engine agent — only if its service isn't installed yet, so app updates
  ;    (which re-run this hook) never re-enrol the PC as a new device.
  nsExec::ExecToStack 'sc.exe query "${LC_SERVICE}"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "Installing the Leeep Connect remote support service..."
    nsExec::ExecToLog '"$INSTDIR\meshagent\meshagent.exe" -fullinstall'
    Pop $0
    ${If} $0 != 0
      DetailPrint "Remote support service install returned $0"
    ${EndIf}
  ${Else}
    DetailPrint "Remote support service already installed"
  ${EndIf}

  ; 2. leeep-connect:// for all users
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}" "" "URL:Leeep Connect"
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}" "URL Protocol" ""
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

  ; 3. Tray app at every login
  WriteRegStr HKLM "${LC_RUN_KEY}" "LeeepConnect" '"$INSTDIR\${MAINBINARYNAME}.exe" --minimized'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Close the tray app (it may be running for any logged-in user)
  nsExec::ExecToLog 'taskkill /F /IM "${MAINBINARYNAME}.exe"'

  ; Updates also run the uninstaller first; keep the agent (and the PC's
  ; enrolment) in that case. $UpdateMode is set by Tauri's template.
  ${If} $UpdateMode <> 1
    DetailPrint "Removing the Leeep Connect remote support service..."
    ${If} ${FileExists} "$INSTDIR\meshagent\meshagent.exe"
      nsExec::ExecToLog '"$INSTDIR\meshagent\meshagent.exe" -fulluninstall'
    ${EndIf}
    DeleteRegKey HKLM "Software\Classes\${LC_PROTOCOL}"
    DeleteRegValue HKLM "${LC_RUN_KEY}" "LeeepConnect"
  ${EndIf}
!macroend
