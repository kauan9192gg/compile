#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::mem::zeroed;
use std::process::Command;
use std::ptr::null_mut;
use std::thread;
use std::time::Duration;

type Hwnd = *mut c_void;

#[repr(C)]
struct Kbd {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        id: i32,
        proc_: unsafe extern "system" fn(i32, usize, isize) -> isize,
        hmod: *mut c_void,
        tid: u32,
    ) -> *mut c_void;
    fn CallNextHookEx(h: *mut c_void, code: i32, w: usize, l: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    fn GetAsyncKeyState(vk: i32) -> i16;
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
    fn SetForegroundWindow(h: Hwnd) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn CreateMutexW(attr: *mut c_void, owner: i32, name: *const u16) -> *mut c_void;
    fn GetLastError() -> u32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn tecla_pressionada(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) < 0 }
}

fn achar_janela() -> Hwnd {
    let t1 = wide("Ferramenta de Captura");
    let t2 = wide("Snipping Tool");
    unsafe {
        let h = FindWindowW(std::ptr::null(), t1.as_ptr());
        if !h.is_null() {
            return h;
        }
        FindWindowW(std::ptr::null(), t2.as_ptr())
    }
}

fn capturar() {
    let mut h = achar_janela();
    if h.is_null() {
        let _ = Command::new("snippingtool.exe").spawn();
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(30));
            h = achar_janela();
            if !h.is_null() {
                break;
            }
        }
    }
    if h.is_null() {
        return;
    }

    // espera soltar Win e Shift, senao o Ctrl+N vira Win+Shift+Ctrl+N
    for _ in 0..40 {
        if !tecla_pressionada(0x5B) && !tecla_pressionada(0x5C) && !tecla_pressionada(0x10) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    unsafe {
        SetForegroundWindow(h);
    }
    thread::sleep(Duration::from_millis(120));
    unsafe {
        keybd_event(0x11, 0, 0, 0); // Ctrl
        keybd_event(0x4E, 0, 0, 0); // N
        keybd_event(0x4E, 0, 2, 0);
        keybd_event(0x11, 0, 2, 0);
    }
}

unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
    if code >= 0 && w == 0x100 {
        let k = &*(l as *const Kbd);
        // 0x53 = S, flags & 0x10 = tecla injetada (ignora as nossas)
        if k.vk_code == 0x53 && k.flags & 0x10 == 0 {
            let win = tecla_pressionada(0x5B) || tecla_pressionada(0x5C);
            let shift = tecla_pressionada(0x10);
            if win && shift {
                // tecla fantasma para o menu Iniciar nao abrir ao soltar o Win
                keybd_event(0xE8, 0, 0, 0);
                keybd_event(0xE8, 0, 2, 0);
                thread::spawn(capturar);
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}

fn main() {
    unsafe {
        // impede abrir duas copias
        let nome = wide("Local\\AtalhoCapturaRust");
        CreateMutexW(null_mut(), 0, nome.as_ptr());
        if GetLastError() == 183 {
            return;
        }

        SetWindowsHookExW(13, hook, GetModuleHandleW(std::ptr::null()), 0);
        let mut m: Msg = zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {}
    }
}
