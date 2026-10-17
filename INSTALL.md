# Portal Keeper

Cafe, dorm or school wifi that kicks you to a login page every few minutes? Portal Keeper
sits in the Windows tray and signs you back in automatically. It does **not** drop the
connection, so your call or video keeps running through the reconnect.

Works with any network you add — a dorm wifi, a hotel, a lab — each with its own username
and password.

## What you need

- Windows 10 or 11
- Python 3 (from [python.org](https://www.python.org/downloads/)) — tick **Add python.exe
  to PATH** in the installer

## Install (about a minute)

1. Unzip this folder somewhere permanent (not Downloads).
2. Double-click **`START.bat`**. The first run installs what it needs; give it a moment.
3. Look for the green circle in the tray — check the hidden `^` icons next to the clock.
4. Right-click the green circle → **Settings (networks)**.
5. Press **Use current Wi-Fi name** (you must be on that network), type your username and
   password, then **Save network**.
6. Add a second network if you have more than one (for example building A vs floor 7).
7. Optional: tick **Start with Windows**.

That's it — from now on it watches and re-logs you in silently.

## How you know it's working

The tray menu shows a status line:

| Status | Meaning |
|---|---|
| `ok` | Connected and signed in. Nothing to do. |
| `logged in` | It just signed you in again |
| `login retry` | The portal refused — check the log |
| `idle` | This wifi isn't in your list; add it in Settings |

## Privacy

Your passwords are saved in **`config.json` on your own PC** and are sent only to the
login page of the network you're signed into. There is no account, no cloud, no telemetry
and no license popup. If you ever want it gone, delete the folder — nothing else is left behind.

## Troubleshooting

- **`login retry` won't clear** — the school probably changed its login page. Open
  `keeper.log` next to the program; the last few lines say which form it tried.
- **Portal URL field** — leave it blank first. Only paste an address if it never
  auto-detects: copy it from the browser when the wifi asks you to sign in.
- **It works on one wifi but not another** — that network needs its own entry in Settings.
- **Weird JavaScript-only login wall** — some portals can't be filled automatically. If
  that's yours, this won't help, and you should be refunded.

Only use it on networks you are allowed to use, with your own account.
