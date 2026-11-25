use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESH: u32 = WM_APP + 2;

static HWND: AtomicIsize = AtomicIsize::new(0);
static ICON_GREEN: AtomicIsize = AtomicIsize::new(0);
static ICON_AMBER: AtomicIsize = AtomicIsize::new(0);
static ICON_GREY: AtomicIsize = AtomicIsize::new(0);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn refresh() {
    let hwnd = HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, WM_REFRESH, 0, 0) };
    }
}

unsafe fn make_icon(rgb: (u8, u8, u8)) -> HICON {
    let dc = CreateCompatibleDC(null_mut());
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = 32;
    bmi.bmiHeader.biHeight = -32;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = null_mut();
    let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if color.is_null() || bits.is_null() {
        DeleteDC(dc);
        return null_mut();
    }
    let px = bits as *mut u8;
    for y in 0..32i32 {
        for x in 0..32i32 {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = ((y * 32 + x) * 4) as usize;
            let dot = (x as f32 - 16.0).powi(2) + (y as f32 - 19.0).powi(2) <= 10.0;
            let (r, g, b) = if dot {
                (255, 255, 255)
            } else {
                (rgb.0, rgb.1, rgb.2)
            };
            let alpha = if dot {
                255.0
            } else {
                ((15.5 - dist) * 255.0).clamp(0.0, 255.0)
            } as u8;
            *px.add(i) = b;
            *px.add(i + 1) = g;
            *px.add(i + 2) = r;
            *px.add(i + 3) = alpha;
        }
    }
    let mask = CreateBitmap(32, 32, 1, 1, null());
    let info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&info);
    DeleteObject(color as HGDIOBJ);
    DeleteObject(mask as HGDIOBJ);
    DeleteDC(dc);
    icon
}

unsafe fn tooltip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let w = wide(text);
    let n = w.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&w[..n]);
}

unsafe fn apply_state(hwnd: HWND) {
    let (enabled, _ssid, msg, ok) = crate::snapshot();
    let icon = if !enabled {
        ICON_GREY.load(Ordering::SeqCst) as HICON
    } else if ok {
        ICON_GREEN.load(Ordering::SeqCst) as HICON
    } else {
        ICON_AMBER.load(Ordering::SeqCst) as HICON
    };
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_TIP;
    nid.hIcon = icon;
    tooltip(&mut nid, &format!("Portal Keeper — {msg}"));
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn open_config() {
    let path = crate::config::path();
    let target = wide(&path.display().to_string());
    ShellExecuteW(
        null_mut(),
        wide("open").as_ptr(),
        target.as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}

unsafe fn toggle_enabled() {
    let mut cfg = crate::config::load();
    cfg.enabled = !cfg.enabled;
    crate::config::save(&cfg);
    crate::status().lock().unwrap().enabled = cfg.enabled;
    crate::config::log(if cfg.enabled { "enabled" } else { "paused" });
    apply_state(HWND.load(Ordering::SeqCst) as HWND);
}

unsafe fn toggle_startup() {
    let mut cfg = crate::config::load();
    cfg.start_with_windows = !cfg.start_with_windows;
    crate::config::save(&cfg);
    crate::startup_set(cfg.start_with_windows);
}

unsafe fn show_menu(hwnd: HWND) {
    let (enabled, ssid, msg, _ok) = crate::snapshot();
    let cfg = crate::config::load();
    let menu = CreatePopupMenu();
    let status_line = wide(&msg);
    let wifi_line = wide(&format!("Wi-Fi: {}", if ssid.is_empty() { "-" } else { &ssid }));
    let toggle = wide(if enabled { "Keeper ON" } else { "Keeper OFF" });
    let test = wide("Test sign-in now");
    let startup = wide("Start with Windows");
    let cfgitem = wide("Open config (accounts)");
    let quit = wide("Quit");

    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, status_line.as_ptr());
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, wifi_line.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(
        menu,
        MF_STRING | if enabled { MF_CHECKED } else { 0 },
        1,
        toggle.as_ptr(),
    );
    AppendMenuW(menu, MF_STRING, 2, test.as_ptr());
    AppendMenuW(
        menu,
        MF_STRING | if cfg.start_with_windows { MF_CHECKED } else { 0 },
        6,
        startup.as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, 3, cfgitem.as_ptr());
    AppendMenuW(menu, MF_STRING, 5, quit.as_ptr());

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    ) as u32;
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);

    match cmd {
        1 => toggle_enabled(),
        2 => {
            crate::FORCE.store(true, Ordering::SeqCst);
        }
        3 => open_config(),
        5 => {
            crate::QUIT.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
        }
        6 => toggle_startup(),
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let ev = lp as u32;
            match ev {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                WM_LBUTTONDBLCLK => open_config(),
                _ => {}
            }
            0
        }
        WM_REFRESH => {
            apply_state(hwnd);
            0
        }
        WM_DESTROY => {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run() {
    unsafe {
        ICON_GREEN.store(make_icon((28, 120, 88)) as isize, Ordering::SeqCst);
        ICON_AMBER.store(make_icon((192, 138, 46)) as isize, Ordering::SeqCst);
        ICON_GREY.store(make_icon((130, 130, 130)) as isize, Ordering::SeqCst);

        let instance = GetModuleHandleW(null());
        let class_name = wide("PortalKeeperWindow");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = instance;
        wc.lpszClassName = class_name.as_ptr();
        RegisterClassW(&wc);

        let title = wide("Portal Keeper");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        HWND.store(hwnd as isize, Ordering::SeqCst);

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = ICON_AMBER.load(Ordering::SeqCst) as HICON;
        tooltip(&mut nid, "Portal Keeper — starting");
        Shell_NotifyIconW(NIM_ADD, &nid);
        apply_state(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
