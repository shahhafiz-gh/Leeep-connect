# Leeep Connect desktop app (`connect/app`)

The Windows tray app a school installs once. A teacher never opens a terminal
or types a code: they click **Leeep Connect** in the school site, the browser
opens this app, the app joins the support queue, and the teacher clicks
**Accept** when the agent connects.

Status: written, **not yet compiled**. The first build needs a Windows machine
with Rust, or the GitHub workflow (`.github/workflows/leeep-connect-windows.yml`).

## How it works

```
school site ── leeep-connect://join?token=… ──▶ app
app ── claim(token, engine id, computer name, version) ──▶ central (admin.leeep.in)
app ── app_status every 3 s ──▶ Waiting → Agent joining → In session → Ended
Stop button / tray Stop / tray Exit ── app_end ──▶ central ends the session
```

- **Central host is fixed at build time.** Production builds talk only to
  `https://admin.leeep.in`. The link only supplies the one-time token; any host
  in it is ignored.
- **Secrets stay in memory.** The per-request device secret is never written
  to disk, the registry or the log.
- **Engine id.** The app reads the engine agent's NodeId from the registry. If
  it can't, central matches the PC by computer name in the engine, and it
  double-checks any id the app sends.
- **Single instance.** A second launch (a new link) is handed to the running app.
- **Updates.** Central pushes new versions through the engine agent (see
  "Releases and auto-update"). The app's own Tauri updater stays off: it has
  no updater public key.
- **Logs.** `%LOCALAPPDATA%\LeeepConnect\logs\`, rotated daily, no tokens.

## Build on Windows

1. Install Rust and the MSVC build tools:
   ```powershell
   winget install Rustlang.Rustup
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   cargo install tauri-cli --version "^2" --locked
   ```
2. Put the engine agent at `connect/installer/resources/meshagent.exe`
   (engine web UI → **Leeep Clients** → **Add Agent** → Windows → *Background &
   interactive* → x64).
3. Build:
   ```powershell
   cd connect\app
   cargo test
   cargo tauri build
   ```
   The installer is `target\release\bundle\nsis\Leeep Connect_<version>_x64-setup.exe`.

### Test build against a local central

The central address is compiled in, so a test build sets it when building:

```powershell
$env:LEEEP_CONNECT_CENTRAL_URL = "http://192.168.1.7:8000"   # where central is reachable from the school PC
$env:LEEEP_CONNECT_SITE_NAME   = "zeplio.localhost"          # Frappe site name on that server
cargo tauri build
```

Never ship a build made with these variables.

## Releases and auto-update

Releases are built on GitHub by `.github/workflows/release.yml`; nothing is
built or uploaded by hand. App updates are pushed by central through the engine
(step 4), so the Tauri updater key (`TAURI_SIGNING_PRIVATE_KEY`) is not needed.

1. One time: set the `LEEEP_CONNECT_AGENT_URL` secret (see
   `.github/workflows/release.yml`), and in **Leeep Connect Settings** on
   admin.leeep.in set *Windows Installer URL* to
   `https://github.com/shahhafiz-gh/Leeep-connect/releases/latest/download/LeeepConnect-setup.exe`.
   That link always serves the newest release, so it never changes again.
2. Each release: bump `version` in `Cargo.toml`, commit, then
   `git tag v<version> && git push origin v<version>`. The **Release** workflow
   builds and publishes it; new downloads get it straight away.
3. To update PCs that already have the app: in **Leeep Connect Settings** set
   *Current App Version* and *Installer SHA-256* (both are in the release notes),
   try one PC with **Update** in the console's Devices tab, then tick *Push
   Updates Through the Engine*. Central has the engine agent (SYSTEM) download
   `LeeepConnect-<version>-setup.exe`, check its SHA-256 and run it with
   `/S /UPDATE`, so keep that file name, and keep `hooks.nsh` leaving the agent
   alone in update mode.

## Code signing

Every exe and the installer must be signed with the OV certificate (decision
D6), or SmartScreen and antivirus will warn schools. The workflow signs through
the `LEEEP_CONNECT_SIGN_COMMAND` secret (Tauri's `bundle.windows.signCommand`,
with `%1` for the file). After the first signed release, submit the installer
to Microsoft (WDSI) and Quick Heal for false-positive review.
