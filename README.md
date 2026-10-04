# Portal Keeper

School / cafe wifi that kicks you to a login page every few minutes. This stays in the Windows tray and signs you back in. It does not drop the wifi, so Zoom and Chrome keep going.

Works for **any** network you add — not one campus hardcoded. Two dorm SSIDs, hotel wifi, lab wifi: each gets its own username and password.

This is the thing you can zip and sell. You are selling setup + a quiet app, not magic.

## Setup (buyer / you)

1. Windows 10/11. Install Python 3, tick **Add python.exe to PATH**.
2. Unzip the folder somewhere it will stay (not the Desktop Downloads pile).
3. Double-click `START.bat`. Wait once while packages install.
4. Green circle in the tray (check the hidden `^` icons).
5. Right-click → **Settings (networks)**.
6. **Use current Wi-Fi name** while you are on that network, type username + password, **Save network**.
7. Repeat if you have a second wifi (example: building A vs floor 7).
8. Optional: **Start with Windows**.

Passwords stay in `config.json` on that PC. They are not uploaded anywhere.

**Portal URL:** leave blank first. Only paste the login page address if it never auto-detects (the URL in the browser when the school asks you to sign in).

## Use

- **Keeper ON / OFF** — pause without quitting
- Status line — `ok`, `logged in`, `login retry`, or `idle` (this wifi is not in your list)
- `keeper.log` if it keeps failing — the school probably changed the form

Only use it on networks you are allowed to use, with **your** account.

## How you actually charge money

Do not wait for an app store.

1. Test it on your campus until `ok` sticks.
2. Zip the folder **without** your `config.json` and **without** `keeper.log`.
3. Sell the zip for about **$5–8** (Gumroad, Telegram, class group). Include this README.
4. Offer a 5-minute install in person for extra (that is often more than the zip).
5. One extra campus = one extra network in Settings, not a new product.

If their portal is a weird JavaScript wall, refund and note it — better than fake “works everywhere.”

## Not included on purpose

No account cloud, no license key, no telemetry. If it feels cheap, it is because license popups look cheaper.
