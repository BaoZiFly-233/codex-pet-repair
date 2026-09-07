use crate::{
    native::{self, Window},
    repair, storage,
};
use std::{
    mem::zeroed,
    os::windows::process::CommandExt,
    path::Path,
    process::Command,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use windows_sys::{
    core::w,
    Win32::{
        Foundation::*,
        System::{LibraryLoader::*, Threading::*},
        UI::WindowsAndMessaging::*,
    },
};
fn pump() {
    unsafe {
        let mut m: MSG = zeroed();
        while PeekMessageW(&mut m, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    DefWindowProcW(h, m, w, l)
}
fn create() -> Result<Window, String> {
    unsafe {
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = GetModuleHandleW(null());
        wc.lpszClassName = w!("PetRepair.TestOverlay");
        RegisterClassW(&wc);
        let h = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            w!("PetRepair.TestOverlay"),
            w!("Synthetic test"),
            WS_POPUP | WS_VISIBLE,
            -32000,
            -32000,
            100,
            100,
            null_mut(),
            null_mut(),
            wc.hInstance,
            null(),
        );
        if h.is_null() {
            return Err(native::error("test CreateWindow"));
        }
        SetLayeredWindowAttributes(h, 0, 1, LWA_ALPHA);
        pump();
        native::scan_test(std::process::id())
            .into_iter()
            .find(|w| w.hwnd == h as usize)
            .ok_or("test discovery failed".into())
    }
}
fn wait(r: &repair::Running, w: &Window, action: &str) -> Result<repair::Report, String> {
    let start = Instant::now();
    let mut acted = false;
    loop {
        pump();
        if start.elapsed() > Duration::from_secs(12) {
            r.cancel();
            return Err("self-test timeout".into());
        }
        if !acted
            && start.elapsed() > Duration::from_millis(700)
            && native::style(w.hwnd).is_ok_and(|s| s & WS_EX_LAYERED == 0)
        {
            acted = true;
            match action {
                "cancel" => r.cancel(),
                "conflict" => {
                    native::set_style(w.hwnd, native::style(w.hwnd)? & !WS_EX_TRANSPARENT)?
                }
                "destroy" => unsafe {
                    DestroyWindow(w.hwnd as HWND);
                },
                "crash" => unsafe {
                    let h = native::Handle(OpenProcess(PROCESS_TERMINATE, 0, r.worker_pid));
                    if h.0.is_null() || TerminateProcess(h.0, 77) == 0 {
                        return Err("could not simulate worker crash".into());
                    }
                },
                _ => {}
            }
        }
        match r.result.try_recv() {
            Ok(result) => return Ok(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("worker disconnected".into())
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn run(path: &Path) {
    let result = std::panic::catch_unwind(run_inner);
    let value = match result {
        Ok(Ok(rows)) => serde_json::json!({"passed":true,"tests":rows}),
        Ok(Err(e)) => serde_json::json!({"passed":false,"error":e}),
        Err(_) => serde_json::json!({"passed":false,"error":"panic"}),
    };
    let _ = storage::write_json(path, &value);
}
fn run_inner() -> Result<Vec<serde_json::Value>, String> {
    let mut rows = Vec::new();
    let window = create()?;
    let classification = (|| -> Result<(), String> {
        let guard = native::TargetGuard::new(&window)?;
        let marker = "PetRepair-classifier-self-test";
        native::marker_set(window.hwnd, marker)?;
        for _ in 0..1000 {
            if !guard.matches(marker) {
                return Err("cached target guard failed".into());
            }
        }
        if guard.matches("wrong-marker") {
            return Err("wrong marker accepted".into());
        }
        let mut owner = window.owner.clone();
        owner.pid = 0;
        if native::inspect_window(&owner, window.hwnd).is_some() {
            return Err("wrong owner accepted".into());
        }
        owner = window.owner.clone();
        owner.test = false;
        if native::inspect_window(&owner, window.hwnd).is_some() {
            return Err("wrong class accepted".into());
        }
        for flag in [WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TOOLWINDOW] {
            native::set_style(window.hwnd, window.style & !flag)?;
            if flag == WS_EX_TOPMOST {
                unsafe {
                    SetWindowPos(
                        window.hwnd as HWND,
                        HWND_NOTOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            if native::inspect_window(&window.owner, window.hwnd).is_some() {
                return Err("wrong style accepted".into());
            }
            native::set_style(window.hwnd, window.style)?;
            unsafe {
                SetWindowPos(
                    window.hwnd as HWND,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        native::set_style(window.hwnd, window.style)?;
        unsafe {
            ShowWindow(window.hwnd as HWND, SW_HIDE);
        }
        if native::inspect_window(&window.owner, window.hwnd).is_some() {
            return Err("hidden window accepted".into());
        }
        unsafe {
            ShowWindow(window.hwnd as HWND, SW_SHOWNOACTIVATE);
        }
        if native::inspect_window(&window.owner, window.hwnd).is_none() {
            return Err("reopened window missing".into());
        }
        native::marker_remove(window.hwnd, marker);
        if guard.matches(marker) {
            return Err("expired transaction accepted".into());
        }
        Ok(())
    })();
    unsafe {
        DestroyWindow(window.hwnd as HWND);
    }
    classification?;
    rows.push(
        serde_json::json!({"scenario":"strict classifier and cached identity", "passed":true}),
    );
    // These are our own off-screen windows; no Codex window is altered.
    for mode in ["normal", "cancel", "conflict", "crash", "destroy"] {
        let w = create()?;
        let outcome = (|| {
            let r = repair::launch(w.clone())?;
            let result = wait(&r, &w, mode)?;
            let actual = native::style(w.hwnd).ok();
            match mode {
                "normal" => {
                    if result.code != "reset" || !result.exact || actual != Some(w.style) {
                        return Err(format!("{mode}: {result:?} actual {actual:?}"));
                    }
                }
                "cancel" => {
                    if result.code != "cancelled_restored"
                        || !result.exact
                        || actual != Some(w.style)
                    {
                        return Err(format!("{mode}: {result:?}"));
                    }
                }
                "conflict" => {
                    if !result.restored
                        || result.exact
                        || actual != Some(w.style & !WS_EX_TRANSPARENT)
                    {
                        return Err(format!("{mode}: {result:?}"));
                    }
                }
                "crash" => {
                    if !result.restored || actual != Some(w.style) {
                        return Err(format!("{mode}: {result:?}"));
                    }
                }
                "destroy" if result.restored || native::same(&w) => {
                    return Err(format!("{mode}: {result:?}"));
                }
                _ => {}
            }
            rows.push(serde_json::json!({"scenario":mode,"report":result}));
            Ok(())
        })();
        unsafe {
            if IsWindow(w.hwnd as HWND) != 0 {
                DestroyWindow(w.hwnd as HWND);
            }
        }
        outcome?;
    }
    let w = create()?;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--test-orphan-parent")
        .arg(serde_json::to_string(&w).unwrap())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut changed = false;
    let mut exited = false;
    let mut restored = false;
    while start.elapsed() < Duration::from_secs(10) {
        pump();
        let s = native::style(w.hwnd)?;
        if s & WS_EX_LAYERED == 0 {
            changed = true;
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            exited = true;
        }
        if exited && changed && s == w.style {
            restored = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        DestroyWindow(w.hwnd as HWND);
    }
    if !restored {
        return Err("parent crash restore failed".into());
    }
    rows.push(serde_json::json!({"scenario":"parent crash","restored":true}));
    thread::sleep(Duration::from_millis(300));
    let _ = repair::recover_pending();
    let w = create()?;
    let t = repair::Transaction {
        window: w.clone(),
        original: w.style,
        marker: format!("PetRepair-test-journal-{}", std::process::id()),
    };
    native::marker_set(w.hwnd, &t.marker)?;
    storage::write_json(&storage::data_dir().join("pending.json"), &t)?;
    native::set_style(w.hwnd, w.style & !WS_EX_LAYERED)?;
    let result = repair::recover_pending().ok_or("journal missing")?;
    let okay = result.exact && native::style(w.hwnd)? == w.style;
    unsafe {
        DestroyWindow(w.hwnd as HWND);
    }
    if !okay {
        return Err("journal recovery failed".into());
    }
    rows.push(serde_json::json!({"scenario":"journal recovery","restored":true}));
    Ok(rows)
}
pub fn orphan_parent(value: &str) {
    let Ok(w) = serde_json::from_str::<Window>(value) else {
        return;
    };
    if !w.owner.test {
        return;
    }
    let Ok(_r) = repair::launch(w.clone()) else {
        return;
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if native::style(w.hwnd).is_ok_and(|s| s & WS_EX_LAYERED == 0) {
            thread::sleep(Duration::from_millis(150));
            std::process::exit(77);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn real_once(path: &Path) {
    let result = (|| -> Result<serde_json::Value, String> {
        let mut d = native::Discovery::default();
        d.refresh_processes();
        let windows = d.scan();
        if windows.len() != 1 {
            return Err("Exactly one official overlay is required".into());
        }
        let w = windows[0].clone();
        let r = repair::launch(w.clone())?;
        let report = wait(&r, &w, "normal")?;
        Ok(
            serde_json::json!({"version":w.owner.version,"report":report,"before":format!("0x{:08X}",w.style),"after":native::style(w.hwnd).ok().map(|s|format!("0x{s:08X}"))}),
        )
    })();
    let value = match result {
        Ok(v) => v,
        Err(e) => serde_json::json!({"error":e}),
    };
    let _ = storage::write_json(path, &value);
}
