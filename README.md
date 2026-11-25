# Portal Keeper

Campus / cafe Wi-Fi that throws you back to a login page. This signs you in quietly in the
background, on Windows and on Android, and never disconnects the Wi-Fi — downloads and calls
keep running.

Two networks, two different accounts: it reads which SSID you are actually associated with and
uses that network's own username and password. Walk from floor 5 to floor 7 and nothing needs
clicking.

## Windows

`portal-keeper.exe` — a single file, no Python, no installer, no runtime.

- 267 KB on disk, ~17 MB RAM, one process, no console window.
- SSID read through the Windows WLAN API and the router address through IP Helper, so there is
  no `netsh` subprocess on every check (that was most of the old lag).
- Checks every 15 s by default and immediately when the SSID changes.
- Signs in by POSTing the hotspot's own form to `http://<gateway>/login`, then proves it worked
  with a real request instead of trusting the portal's reply. Measured on campus: signed out →
  signed in in about 4 seconds.
- Tray icon: green = signed in, amber = retrying, grey = paused. Right-click for status,
  Keeper ON/OFF, test sign-in now, start with Windows, open config, quit.
- Accounts live in `config.json` next to the exe (see `config.example.json`). Edits are picked
  up without a restart.

## Android

`PortalKeeper.apk` — 20 KB, plain platform Java, no AndroidX and no Play services.

- Foreground service, checks every 10 s, starts again after a reboot.
- Binds its own requests to the Wi-Fi network, so mobile data cannot mask a signed-out portal.
- Location permission is needed to read the Wi-Fi name, and the service declares a location
  foreground type so the name stays readable with the screen off (otherwise the OS redacts it and
  the app cannot tell which account to use).
- Allow background/battery once, or the phone will kill the service.

The APK is pre-seeded with the two campus accounts and they can be edited in the app.

Verified on a HyperOS 2 phone on `KBU-FL7`: service foreground, notification reading
`Signed in · KBU-FL7`, Wi-Fi name still readable with the screen off.

## How the sign-in works

`POST http://<gateway>/login` with `username`, `password`, `dst` and `popup` — the same fields
the hotspot page submits. Success is confirmed by `connectivitycheck.gstatic.com/generate_204`
answering 204.

Only use it on networks you are allowed to use, with your own account.

## Layout

| path | what |
| --- | --- |
| `win/` | Rust source, `cargo build --release` |
| `android/` | Java source and `build.sh` (aapt2 + javac + d8 + apksigner, no Gradle) |
| `legacy-python/` | the old Python/pystray version, kept for reference |
