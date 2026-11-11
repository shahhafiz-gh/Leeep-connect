# Leeep Connect installer (`connect/installer`)

The school installer is built by Tauri (`cargo tauri build` in `connect/app`),
not by a separate NSIS script. This folder holds what Tauri's installer adds:

| File | What it is |
|---|---|
| `hooks.nsh` | Install/uninstall steps added to Tauri's NSIS installer |
| `resources/meshagent.exe` | The engine agent for the **Leeep Clients** group. Not in git: it embeds one server's address, so it's downloaded per build (see `connect/app/README.md`) |

## What a school sees

1. Download **Leeep Connect** from the support dialog and open it.
2. Windows asks "Allow this app to make changes?" → **Yes**. (Required once:
   the remote-support service is a Windows service.)
3. Click **Install**. Done. Nothing to configure.

## What the installer does

- Installs the app for all users in `C:\Program Files\Leeep Connect`.
- Installs and enrols the engine agent as the **LeeepConnect** Windows service.
  Skipped when the service already exists, so updates don't create a duplicate
  device.
- Registers `leeep-connect://` for all users (HKLM).
- Starts the app in the tray at every login.

Uninstalling closes the app, removes the agent service and both registry
entries. During auto-updates the agent is kept, so the PC keeps its identity.

## Still to verify on a real build (marked `VERIFY` in `hooks.nsh`)

- The agent's `-fullinstall` / `-fulluninstall` switches and its service name.
- Tauri's template variables `MAINBINARYNAME` and `$UpdateMode`.
- A clean install, an update, and an uninstall on Windows 10 and Windows 11,
  each leaving no trace afterwards.
