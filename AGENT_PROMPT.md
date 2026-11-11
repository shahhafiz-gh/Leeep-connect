# Task: build, install and test the Leeep Connect Windows installer on this PC

You are a coding agent on a Windows 11 PC (computer name **Aech**, LAN IP **192.168.1.8**).
The user handed you this folder (`leeep-connect`) and this prompt. Do the work below,
fix build problems in the code if you hit them, and finish with the report at the end.

## What this is

Leeep Connect is a remote-support tool for schools. A school installs one Windows
installer that puts two things on the PC:

1. **The Leeep Connect tray app** (Tauri 2 + Rust, in `app/`). A teacher clicks
   "Leeep Connect" in the school website; the browser opens a `leeep-connect://join?token=…`
   link; the app claims the request with the central server and waits for support.
2. **The MeshCentral agent** (`installer/resources/meshagent.exe`), installed as the
   Windows service **LeeepConnect** by the NSIS hooks in `installer/hooks.nsh`.
   Support controls the screen through it after the person at the PC clicks **Allow**.

This is the **first ever build** of the app. Expect some compile errors; fixing them
is part of the job. Read `app/README.md` and `installer/README.md` first.

## The test network (fixed facts)

| What | Where |
|---|---|
| Central server (Frappe), the app talks to it | `http://192.168.1.7:8000`, Frappe site name `zeplio.localhost` (sent as the `X-Frappe-Site-Name` header; the app already does this when built with the env vars below) |
| School website (to start a request) | `http://dev.leeep.in:8000/leeep`, served by 192.168.1.7 (needs a hosts-file line on this PC, step 2) |
| MeshCentral server | runs on **this PC**: `https://localhost:4430` (self-signed cert), device group **Leeep Clients** |
| Support console (on the other PC) | the user drives it; you don't need it |

## Rules

- **Ask the user before** anything that needs admin rights or changes the system
  (hosts file, uninstalling the old agent, running the installer). Say exactly what
  you're about to do.
- **Never type or ask for passwords or keys in chat.** If something needs the
  MeshCentral login, ask the user to do that step in the browser themselves.
- Don't weaken security to make things work: the app must keep reading the central
  address **only** from the build-time constant (never from the link), the device
  secret must stay in memory only, and the Allow/consent prompt must stay on.
- Don't change the central server or MeshCentral settings.
- This build bakes in a LAN test address. It's for this test only; never publish it.
- Keep a list of every file you change and why. Save all your code changes as one
  patch file at the end (step 9).

## Steps

### 1. Check the toolchain
`rustc --version`, `cargo --version`, `cargo tauri --version`. If the Tauri CLI is
missing: `cargo install tauri-cli --version "^2" --locked`. MSVC C++ build tools must
be present (the user says they are).

### 2. Make the central server and school site reachable
1. Test central:
   `Invoke-RestMethod http://192.168.1.7:8000/api/method/ping -Headers @{ 'X-Frappe-Site-Name' = 'zeplio.localhost' }`
   must return `pong`. If it can't connect, **stop and tell the user**: the other PC
   (192.168.1.7) still has to open port 8000 to this PC. Don't try to fix that from here.
2. With the user's OK, add this line to `C:\Windows\System32\drivers\etc\hosts`
   (admin needed), then check that `http://dev.leeep.in:8000/leeep` opens in the browser:
   ```
   192.168.1.7   dev.leeep.in
   ```

### 3. Remove the MeshCentral agent that's already installed
This PC already has a MeshCentral agent (installed by hand earlier). The test is whether
**our installer** installs the agent, so remove the old one first:
1. `Get-Service | Where-Object { $_.Name -match 'mesh|leeep' }` and tell the user what you found.
2. With the user's OK, uninstall it (run its exe with `-fulluninstall`, e.g.
   `& "C:\Program Files\Mesh Agent\MeshAgent.exe" -fulluninstall`; find the real path
   from the service's ImagePath).
3. Ask the user to delete the old, now offline, **Aech** device in the MeshCentral web
   page (device → Delete Device), so only the new one shows later.

### 4. Get the agent file for the build
Ask the user to download it in MeshCentral (they're logged in; you're not):
**My Devices → Leeep Clients → Add Agent → Windows → "Background & interactive" →
download the x64 executable**. Save it as `installer\resources\meshagent.exe`
(rename it; MeshCentral may name it `leeep-connect-agent.exe`). Check that it's over 1 MB.

### 5. Build
Keep the folder layout (`app` and `installer` side by side; `tauri.conf.json` uses `../installer/...`).
```powershell
cd <this folder>\app
$env:LEEEP_CONNECT_CENTRAL_URL = "http://192.168.1.7:8000"
$env:LEEEP_CONNECT_SITE_NAME   = "zeplio.localhost"
cargo test
cargo tauri build
```
Output: `app\target\release\bundle\nsis\Leeep Connect_<version>_x64-setup.exe`.
Fix compile errors in the code as needed. After the build, open the generated
`installer.nsi` (under `app\target\release\nsis\`) and confirm that `MAINBINARYNAME`
and `$UpdateMode`, which `hooks.nsh` relies on, really exist in Tauri's template.
If they don't, fix `hooks.nsh` and rebuild.

### 6. Install
With the user's OK, run the setup exe. It's unsigned, so Windows SmartScreen will say
"Windows protected your PC": the user clicks **More info → Run anyway**, then **Yes**
on the admin prompt, then **Install**.

Then check, and record each result:
- `Get-Service LeeepConnect` shows **Running** (the agent service; `hooks.nsh` assumes
  this name from MeshCentral's `agentCustomization.serviceName`; if the real service
  name differs, note it and fix `hooks.nsh` and `app/src/api.rs`).
- Registry `HKLM\SOFTWARE\Open Source\LeeepConnect` has a `NodeId` value (the app
  reads it in `get_mesh_node_id()` in `app/src/api.rs`). Note the real key path and
  value format if different.
- `HKLM\Software\Classes\leeep-connect\shell\open\command` points at the installed app exe.
- `HKLM\Software\Microsoft\Windows\CurrentVersion\Run` has a `LeeepConnect` value.
- The Leeep Connect icon is in the system tray.
- Ask the user: in the MeshCentral page, does **Aech** show **online** in **Leeep Clients**?

### 7. End-to-end test (with the user)
1. The user opens `http://dev.leeep.in:8000/leeep`, logs in as a **Moonland School**
   user, opens **Support → Leeep Connect**, types a reason and starts it.
2. The browser asks to open Leeep Connect → **Open**. The app window should show
   **Waiting**.
3. On the other PC, the support person clicks **Take Session** in the console.
   A prompt appears on this PC ("Leeep support … wants to see and control your screen.
   Allow?") → the user clicks **Allow**. The support console should now show this
   screen and be able to move the mouse.
4. The user clicks **Stop** in the app. The app should show the session ended.

If the app shows an error, read the log in `%LOCALAPPDATA%\LeeepConnect\logs\`
(it never contains tokens) and fix the cause if it's in the app.

### 8. Uninstall test
With the user's OK, uninstall **Leeep Connect** from Windows Settings → Apps. Check
that the `LeeepConnect` service, the `leeep-connect` registry key and the `Run` value
are all gone, and that `C:\Program Files\Leeep Connect` is removed. Then **reinstall
it** (step 6) so the PC is ready for more testing.

### 9. Save your changes
If you changed any files, save one patch the user can bring back to the main repo.
From the folder that contains `app` and `installer`:
```powershell
git init -q; git add -A; git commit -qm base   # only if you didn't do this BEFORE editing
```
(Better: run that **before your first edit**, then at the end
`git diff > leeep-connect-fixes.patch`.) Don't include `target/` or `meshagent.exe`.

## Report back (the user will paste it to the main developer)

1. Toolchain versions.
2. Build result, with every error you hit and how you fixed it (file + reason).
3. Each check in step 6: pass or fail, with the real values (service name, registry
   path, NodeId format).
4. The step 7 test: what worked and where it stopped, with exact error text and the
   relevant log lines.
5. The step 8 uninstall result.
6. The path of `leeep-connect-fixes.patch` (or "no code changes").
