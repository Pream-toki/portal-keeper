"""Campus / cafe captive-portal keeper. Any SSID. Accounts stay on this PC."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
import traceback
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urljoin, urlparse

import requests
import pystray
from PIL import Image, ImageDraw

APP_DIR = Path(__file__).resolve().parent
CONFIG_PATH = APP_DIR / "config.json"
LOG_PATH = APP_DIR / "keeper.log"
STARTUP_NAME = "PortalKeeper.vbs"
PROBE_URLS = (
    "http://connectivitycheck.gstatic.com/generate_204",
    "http://www.msftconnecttest.com/connecttest.txt",
    "http://neverssl.com/",
)

stop_event = threading.Event()
state_lock = threading.Lock()
runtime = {"enabled": True, "last": "starting", "ssid": ""}


def log(msg: str) -> None:
    line = time.strftime("%Y-%m-%d %H:%M:%S ") + msg
    try:
        with LOG_PATH.open("a", encoding="utf-8") as f:
            f.write(line + "\n")
    except OSError:
        pass


def default_config() -> dict:
    return {
        "enabled": True,
        "check_seconds": 20,
        "networks": {
            "Your-WiFi-Name": {
                "username": "",
                "password": "",
                "portal_url": "",
            }
        },
    }


def load_config() -> dict:
    if not CONFIG_PATH.exists():
        save_config(default_config())
    try:
        data = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    except Exception:
        data = default_config()
    data.setdefault("enabled", True)
    data.setdefault("check_seconds", 20)
    data.setdefault("networks", {})
    return data


def save_config(data: dict) -> None:
    CONFIG_PATH.write_text(json.dumps(data, indent=2), encoding="utf-8")


def startup_path() -> Path:
    return (
        Path(os.environ.get("APPDATA", ""))
        / "Microsoft/Windows/Start Menu/Programs/Startup"
        / STARTUP_NAME
    )


def set_startup(icon, want: bool) -> None:
    p = startup_path()
    script = Path(__file__).resolve()
    try:
        if want:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(
                'Set sh = CreateObject("WScript.Shell")\n'
                f'sh.CurrentDirectory = "{script.parent}"\n'
                f'sh.Run "pythonw ""{script}""", 0, False\n',
                encoding="ascii",
            )
        elif p.exists():
            p.unlink()
        cfg = load_config()
        cfg["start_with_windows"] = want
        save_config(cfg)
        icon.update_menu()
    except OSError:
        pass


def current_ssid() -> str:
    try:
        out = subprocess.check_output(
            ["netsh", "wlan", "show", "interfaces"],
            creationflags=subprocess.CREATE_NO_WINDOW,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            errors="ignore",
        )
    except Exception:
        return ""
    for line in out.splitlines():
        if "SSID" in line and "BSSID" not in line:
            parts = line.split(":", 1)
            if len(parts) == 2:
                return parts[1].strip()
    return ""


class FormParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.forms: list[dict] = []
        self._cur: dict | None = None

    def handle_starttag(self, tag: str, attrs) -> None:
        a = dict(attrs)
        if tag == "form":
            self._cur = {
                "action": a.get("action", ""),
                "method": a.get("method", "post").lower(),
                "inputs": {},
            }
            self.forms.append(self._cur)
        elif tag in ("input", "select") and self._cur is not None:
            name = a.get("name")
            if name:
                self._cur["inputs"][name] = a.get("value", "")

    def handle_endtag(self, tag: str) -> None:
        if tag == "form":
            self._cur = None


def looks_like_portal(resp: requests.Response) -> bool:
    if resp.status_code in (401, 302, 303, 307, 308):
        return True
    if resp.status_code == 204:
        return False
    text = (resp.text or "")[:8000].lower()
    if "password" in text and ("login" in text or "username" in text or "user" in text):
        return True
    host = urlparse(resp.url).hostname or ""
    if host.endswith("gstatic.com") or host.endswith("msftconnecttest.com"):
        return resp.status_code not in (200, 204)
    return "login" in text or "captive" in text or "auth" in text


def fill_credentials(inputs: dict, username: str, password: str) -> dict:
    data = dict(inputs)
    user_keys, pass_keys = [], []
    for k in data:
        lk = k.lower()
        if any(x in lk for x in ("pass", "pwd", "password")):
            pass_keys.append(k)
        elif any(x in lk for x in ("user", "uid", "account", "login", "name", "id")):
            user_keys.append(k)
    data[user_keys[0] if user_keys else "username"] = username
    if not user_keys:
        data["user"] = username
    data[pass_keys[0] if pass_keys else "password"] = password
    if not pass_keys:
        data["passwd"] = password
    return data


def try_login(session: requests.Session, username: str, password: str, portal_url: str = "") -> bool:
    headers = {"User-Agent": "Mozilla/5.0 PortalKeeper/1.0"}
    start = None
    urls = ([portal_url] if portal_url else []) + list(PROBE_URLS)
    for url in urls:
        if not url:
            continue
        try:
            start = session.get(url, timeout=8, allow_redirects=True, headers=headers)
            break
        except requests.RequestException:
            continue
    if start is None:
        return False
    if not portal_url and not looks_like_portal(start):
        return True

    parser = FormParser()
    try:
        parser.feed(start.text or "")
    except Exception:
        parser.forms = []
    forms = parser.forms or [{"action": start.url, "method": "post", "inputs": {}}]
    for form in forms:
        action = urljoin(start.url, form.get("action") or "")
        payload = fill_credentials(form.get("inputs") or {}, username, password)
        try:
            if form.get("method") == "get":
                session.get(action, params=payload, timeout=12, headers=headers)
            else:
                session.post(action, data=payload, timeout=12, headers=headers)
            chk = session.get(PROBE_URLS[0], timeout=8, allow_redirects=True, headers=headers)
            if not looks_like_portal(chk) or chk.status_code == 204:
                return True
        except requests.RequestException:
            continue
    return False


def online() -> bool:
    try:
        r = requests.get(PROBE_URLS[0], timeout=6, allow_redirects=True)
        return r.status_code == 204 or not looks_like_portal(r)
    except requests.RequestException:
        return False


def worker() -> None:
    session = requests.Session()
    session.trust_env = False
    while not stop_event.is_set():
        try:
            cfg = load_config()
            enabled = bool(cfg.get("enabled", True))
            ssid = current_ssid()
            nets = cfg.get("networks") or {}
            wait = max(8, int(cfg.get("check_seconds") or 20))
            with state_lock:
                runtime["enabled"] = enabled
                runtime["ssid"] = ssid
            if not enabled:
                runtime["last"] = "paused"
            elif ssid not in nets:
                runtime["last"] = f"idle ({ssid or 'no wifi'})"
            else:
                creds = nets[ssid] or {}
                user = str(creds.get("username") or "")
                pw = str(creds.get("password") or "")
                portal = str(creds.get("portal_url") or "")
                if not user or not pw:
                    runtime["last"] = "add login in Settings"
                elif online():
                    runtime["last"] = f"ok {ssid}"
                else:
                    log(f"login {ssid} as {user}")
                    ok = try_login(session, user, pw, portal)
                    runtime["last"] = "logged in" if ok else "login retry"
                    log("ok" if ok else "fail")
            stop_event.wait(wait)
        except Exception:
            log(traceback.format_exc())
            stop_event.wait(15)


def open_settings(_i=None, _item=None) -> None:
    flags = 0
    if hasattr(subprocess, "CREATE_NO_WINDOW"):
        flags = subprocess.DETACHED_PROCESS
    try:
        subprocess.Popen(
            [sys.executable.replace("pythonw.exe", "python.exe"), str(Path(__file__).resolve()), "settings"],
            cwd=str(APP_DIR),
            creationflags=flags,
        )
    except OSError:
        os.startfile(str(CONFIG_PATH))


def set_enabled(icon, value: bool) -> None:
    cfg = load_config()
    cfg["enabled"] = value
    save_config(cfg)
    with state_lock:
        runtime["enabled"] = value
        runtime["last"] = "on" if value else "paused"
    try:
        icon.update_menu()
    except Exception:
        pass


def quit_app(icon, _item=None) -> None:
    stop_event.set()
    icon.stop()


def menu(icon):
    with state_lock:
        en = runtime["enabled"]
        last = runtime["last"]
        ssid = runtime["ssid"]
    return pystray.Menu(
        pystray.MenuItem(f"Status: {last}", None, enabled=False),
        pystray.MenuItem(f"Wi-Fi: {ssid or '-'}", None, enabled=False),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem("Keeper ON", lambda i, _: set_enabled(i, True), checked=lambda _: en, radio=True),
        pystray.MenuItem("Keeper OFF", lambda i, _: set_enabled(i, False), checked=lambda _: not en, radio=True),
        pystray.MenuItem(
            "Start with Windows",
            lambda i, _: set_startup(i, not startup_path().exists()),
            checked=lambda _: startup_path().exists(),
        ),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem("Settings (networks)", open_settings),
        pystray.MenuItem("Quit", quit_app),
    )


def icon_img():
    img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.ellipse((4, 4, 60, 60), fill=(28, 120, 88, 255))
    d.arc((16, 22, 48, 54), 200, 340, fill="white", width=4)
    d.ellipse((28, 40, 36, 48), fill="white")
    return img


def show_settings() -> None:
    import tkinter as tk
    from tkinter import messagebox, ttk

    cfg = load_config()
    nets = dict(cfg.get("networks") or {})

    root = tk.Tk()
    root.title("Portal Keeper — networks")
    root.geometry("520x420")
    root.minsize(480, 380)

    hint = tk.Label(
        root,
        text="Add the wifi name exactly as Windows shows it.\nPortal URL is optional — leave blank unless login never triggers.",
        justify="left",
        anchor="w",
    )
    hint.pack(fill="x", padx=12, pady=(12, 6))

    cols = ("ssid", "user")
    tree = ttk.Treeview(root, columns=cols, show="headings", height=8)
    tree.heading("ssid", text="Wi-Fi name")
    tree.heading("user", text="Username")
    tree.pack(fill="both", expand=True, padx=12)

    def refresh():
        tree.delete(*tree.get_children())
        for ssid, cred in nets.items():
            tree.insert("", "end", iid=ssid, values=(ssid, cred.get("username", "")))

    form = ttk.Frame(root)
    form.pack(fill="x", padx=12, pady=8)
    labels = ["Wi-Fi name", "Username", "Password", "Portal URL (optional)"]
    vars_ = [tk.StringVar() for _ in labels]
    for i, lab in enumerate(labels):
        ttk.Label(form, text=lab).grid(row=i, column=0, sticky="w", pady=2)
        show = "*" if i == 2 else ""
        ttk.Entry(form, textvariable=vars_[i], show=show, width=44).grid(row=i, column=1, sticky="ew", pady=2)
    form.columnconfigure(1, weight=1)

    def load_sel(_e=None):
        sel = tree.selection()
        if not sel:
            return
        ssid = sel[0]
        c = nets.get(ssid) or {}
        vars_[0].set(ssid)
        vars_[1].set(c.get("username", ""))
        vars_[2].set(c.get("password", ""))
        vars_[3].set(c.get("portal_url", ""))

    tree.bind("<<TreeviewSelect>>", load_sel)

    def use_current():
        s = current_ssid()
        if s:
            vars_[0].set(s)
        else:
            messagebox.showinfo("Portal Keeper", "Windows is not on wifi right now.")

    def save_row():
        ssid = vars_[0].get().strip()
        if not ssid:
            messagebox.showwarning("Portal Keeper", "Wi-Fi name is required.")
            return
        nets[ssid] = {
            "username": vars_[1].get().strip(),
            "password": vars_[2].get(),
            "portal_url": vars_[3].get().strip(),
        }
        cfg["networks"] = nets
        save_config(cfg)
        refresh()
        tree.selection_set(ssid)

    def delete_row():
        ssid = vars_[0].get().strip()
        if ssid in nets:
            del nets[ssid]
            cfg["networks"] = nets
            save_config(cfg)
            refresh()
            for v in vars_:
                v.set("")

    btns = ttk.Frame(root)
    btns.pack(fill="x", padx=12, pady=(0, 12))
    ttk.Button(btns, text="Use current Wi-Fi name", command=use_current).pack(side="left")
    ttk.Button(btns, text="Save network", command=save_row).pack(side="right")
    ttk.Button(btns, text="Remove", command=delete_row).pack(side="right", padx=6)

    refresh()
    root.mainloop()


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "settings":
        show_settings()
        return
    load_config()
    threading.Thread(target=worker, daemon=True).start()
    icon = pystray.Icon(
        "PortalKeeper",
        icon_img(),
        "Portal Keeper",
        menu=pystray.Menu(lambda: menu(icon)),
    )
    icon.run()


if __name__ == "__main__":
    try:
        main()
    except Exception:
        log(traceback.format_exc())
        if len(sys.argv) <= 1:
            worker()
