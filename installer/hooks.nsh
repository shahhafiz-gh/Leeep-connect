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
; Checked on the first build (tauri-cli 2.12.4): MAINBINARYNAME ("leeep-connect")
; and $UpdateMode (set from /UPDATE in un.onInit) exist in Tauri's NSIS template,
; and this pre-uninstall hook runs before the template deletes any files.

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

  ; The installer is a 32-bit process and Tauri's template never calls
  ; SetRegView 64, so HKLM\Software\...\Run would land in WOW6432Node.
  ; Use the 64-bit view for our keys only, then restore Tauri's view.
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}

  ; 2. leeep-connect:// for all users
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}" "" "URL:Leeep Connect"
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}" "URL Protocol" ""
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr HKLM "Software\Classes\${LC_PROTOCOL}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

  ; 3. No login auto-start. In the approval-gated model the app is launched
  ;    only when a support request is approved (via the leeep-connect:// handler
  ;    above) and quits itself when the session ends, so nothing of Leeep's runs
  ;    on the client between sessions. (Any Run value from an older build is
  ;    cleared below so upgrades don't leave it behind.)
  DeleteRegValue HKLM "${LC_RUN_KEY}" "LeeepConnect"

  ${If} ${RunningX64}
    SetRegView lastused
  ${EndIf}
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
    ${If} ${RunningX64}
      SetRegView 64
    ${EndIf}
    DeleteRegKey HKLM "Software\Classes\${LC_PROTOCOL}"
    DeleteRegValue HKLM "${LC_RUN_KEY}" "LeeepConnect"
    ${If} ${RunningX64}
      SetRegView lastused
    ${EndIf}
  ${EndIf}
!macroend
