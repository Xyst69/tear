//! A small native password prompt for `tearw`, built from standard Win32 controls
//! so it matches the system look (themed through the embedded manifest).
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_WINDOW, CreateFontIndirectW, DeleteObject, GetMonitorInfoW, GetSysColor,
    GetSysColorBrush, HDC, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    SetBkColor, SetTextColor,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::{SS_EDITCONTROL, SS_NOPREFIX};
use windows_sys::Win32::UI::Controls::{
    BST_CHECKED, EM_GETPASSWORDCHAR, EM_SETPASSWORDCHAR, ICC_STANDARD_CLASSES,
    INITCOMMONCONTROLSEX, InitCommonControlsEx,
};
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForSystem, SystemParametersInfoForDpi,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW,
    DC_HASDEFID, DM_GETDEFID, DefWindowProcW, DestroyWindow, DispatchMessageW, EN_CHANGE,
    ES_AUTOHSCROLL, ES_PASSWORD, GetCursorPos, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    HMENU, IDC_ARROW, IDCANCEL, IDOK, IsDialogMessageW, LoadCursorW, MSG, NONCLIENTMETRICSW,
    PostQuitMessage, RegisterClassExW, SPI_GETNONCLIENTMETRICS, SW_SHOWNORMAL, SendMessageW,
    SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_STYLE, WM_CLOSE,
    WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_SETFONT, WNDCLASSEXW, WS_CAPTION, WS_CHILD,
    WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const ID_EDIT: i32 = 100;
const ID_SHOW: i32 = 101;
const ERROR_RED: u32 = 0x001C_2BC4; // COLORREF for RGB(196, 43, 28)

#[derive(Clone, Copy)]
struct Controls {
    edit: HWND,
    show: HWND,
    error: HWND,
    mask: usize,
}

thread_local! {
    static CONTROLS: Cell<Controls> = const {
        Cell::new(Controls { edit: null_mut(), show: null_mut(), error: null_mut(), mask: 0 })
    };
    static RESULT: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn window_text(hwnd: HWND) -> String {
    // SAFETY (caller): hwnd is a live window owned by this thread.
    unsafe {
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buffer = vec![0u16; len + 1];
        let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0);
        let text = String::from_utf16_lossy(&buffer[..copied as usize]);
        buffer.fill(0);
        text
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Windows calls this on the UI thread with valid handles for this window.
    unsafe {
        let c = CONTROLS.get();
        match msg {
            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as i32;
                let code = ((wparam >> 16) & 0xFFFF) as u32;
                if id == IDOK {
                    let text = window_text(c.edit);
                    if text.is_empty() {
                        SetWindowTextW(c.error, wide("Enter the password.").as_ptr());
                        SetFocus(c.edit);
                    } else {
                        RESULT.with(|r| *r.borrow_mut() = Some(text));
                        DestroyWindow(hwnd);
                    }
                    return 0;
                }
                if id == IDCANCEL {
                    DestroyWindow(hwnd);
                    return 0;
                }
                if id == ID_SHOW && code == BN_CLICKED {
                    let visible = SendMessageW(c.show, BM_GETCHECK, 0, 0) as u32 == BST_CHECKED;
                    SendMessageW(
                        c.edit,
                        EM_SETPASSWORDCHAR,
                        if visible { 0 } else { c.mask },
                        0,
                    );
                    InvalidateRect(c.edit, null(), 1);
                    SetFocus(c.edit);
                    return 0;
                }
                if id == ID_EDIT && code == EN_CHANGE {
                    SetWindowTextW(c.error, wide("").as_ptr());
                    return 0;
                }
            }
            // Lets IsDialogMessage map Enter to the default button.
            DM_GETDEFID => return ((DC_HASDEFID as LRESULT) << 16) | IDOK as LRESULT,
            WM_CTLCOLORSTATIC => {
                let hdc = wparam as HDC;
                if lparam as HWND == c.error {
                    SetTextColor(hdc, ERROR_RED);
                }
                SetBkColor(hdc, GetSysColor(COLOR_WINDOW));
                return GetSysColorBrush(COLOR_WINDOW) as LRESULT;
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                return 0;
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Asks for an archive password. `action` is the verb on the confirm button,
/// such as "Extract". `notice` is shown in red, for example after a wrong attempt.
/// Returns `None` when the user cancels.
pub fn prompt(archive_name: &str, action: &str, notice: Option<&str>) -> Option<String> {
    RESULT.with(|r| r.borrow_mut().take());
    // SAFETY: standard Win32 window creation and message loop on this thread.
    // All strings passed to the API are NUL-terminated and outlive each call.
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&icc);
        let instance = GetModuleHandleW(null());
        let class = wide("TearPasswordPrompt");
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(window_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.hbrBackground = GetSysColorBrush(COLOR_WINDOW);
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc); // Fails harmlessly if a previous prompt registered it.

        let dpi = GetDpiForSystem();
        let s = |v: i32| v * dpi as i32 / 96;
        let mut metrics: NONCLIENTMETRICSW = std::mem::zeroed();
        metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            (&mut metrics as *mut NONCLIENTMETRICSW).cast(),
            0,
            dpi,
        );
        let font = CreateFontIndirectW(&metrics.lfMessageFont);

        // Client area in 96-DPI units: 380 x 214.
        let style: WINDOW_STYLE = WS_CAPTION | WS_SYSMENU;
        let ex_style = WS_EX_DLGMODALFRAME;
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: s(380),
            bottom: s(214),
        };
        AdjustWindowRectExForDpi(&mut frame, style, 0, ex_style, dpi);
        let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
        // Centre on the monitor where the user right-clicked.
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        );
        let work = monitor.rcWork;
        let x = work.left + (work.right - work.left - width) / 2;
        let y = work.top + (work.bottom - work.top - height) / 2;

        let hwnd = CreateWindowExW(
            ex_style,
            class.as_ptr(),
            wide("Tear").as_ptr(),
            style,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            DeleteObject(font);
            return None;
        }

        let child = |ex: u32, class: &str, text: &str, style: u32, rect: [i32; 4], id: i32| {
            let control = CreateWindowExW(
                ex,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                s(rect[0]),
                s(rect[1]),
                s(rect[2]),
                s(rect[3]),
                hwnd,
                id as isize as HMENU,
                instance,
                null(),
            );
            SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
            control
        };

        let message = format!(
            "\u{201C}{archive_name}\u{201D} is password-protected. Enter its password to continue."
        );
        child(
            0,
            "STATIC",
            &message,
            SS_NOPREFIX | SS_EDITCONTROL,
            [16, 16, 348, 54],
            0,
        );
        let edit = child(
            WS_EX_CLIENTEDGE,
            "EDIT",
            "",
            (ES_PASSWORD | ES_AUTOHSCROLL) as u32 | WS_TABSTOP,
            [16, 78, 348, 26],
            ID_EDIT,
        );
        let show = child(
            0,
            "BUTTON",
            "Show password",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            [16, 112, 200, 20],
            ID_SHOW,
        );
        let error = child(
            0,
            "STATIC",
            notice.unwrap_or(""),
            SS_NOPREFIX,
            [16, 136, 348, 20],
            0,
        );
        child(
            0,
            "BUTTON",
            action,
            BS_DEFPUSHBUTTON as u32 | WS_TABSTOP,
            [180, 170, 88, 28],
            IDOK,
        );
        child(
            0,
            "BUTTON",
            "Cancel",
            BS_PUSHBUTTON as u32 | WS_TABSTOP,
            [276, 170, 88, 28],
            IDCANCEL,
        );
        let mask = SendMessageW(edit, EM_GETPASSWORDCHAR, 0, 0) as usize;
        CONTROLS.set(Controls {
            edit,
            show,
            error,
            mask,
        });

        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
        SetFocus(edit);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        DeleteObject(font);
    }
    RESULT.with(|r| r.borrow_mut().take())
}
