//! Local, same-installation IPC. The native owner serializes all repair state changes.
use crate::native::{self, wide, Handle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::io::FromRawHandle,
    ptr::null,
    sync::mpsc,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
    UI::WindowsAndMessaging::*,
};

pub const MESSAGE: u32 = WM_APP + 10;
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub command: String,
    pub enabled: Option<bool>,
}
pub struct Delivery {
    pub request: Request,
    pub reply: mpsc::Sender<Value>,
}
pub fn name() -> String {
    format!("CodexPetRepair-v2-{}", session())
}
pub fn session() -> u32 {
    let mut session = 0;
    unsafe {
        windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
            GetCurrentProcessId(),
            &mut session,
        );
    }
    session
}
pub fn ui_path() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("ui")
        .join("PetRepair.UI.exe")
}
fn read_line(file: &mut File) -> Result<String, String> {
    let mut data = Vec::new();
    let mut b = [0u8; 1];
    while data.len() < 16384 {
        if file.read(&mut b).map_err(|e| e.to_string())? == 0 {
            return Err("pipe_closed".into());
        }
        if b[0] == b'\n' {
            return String::from_utf8(data).map_err(|e| e.to_string());
        }
        data.push(b[0]);
    }
    Err("request_too_large".into())
}
unsafe fn allowed(pipe: HANDLE) -> bool {
    let mut pid = 0;
    if GetNamedPipeClientProcessId(pipe, &mut pid) == 0 {
        return false;
    }
    let Ok(process) = Handle::process(pid) else {
        return false;
    };
    let Ok(path) = native::process_path(process.0) else {
        return false;
    };
    let own = std::env::current_exe().unwrap();
    path.eq_ignore_ascii_case(&own.to_string_lossy())
        || path.eq_ignore_ascii_case(&ui_path().to_string_lossy())
}
pub fn listen(hwnd: usize) -> Result<(), String> {
    let path = wide(&format!("\\\\.\\pipe\\{}", name()));
    // Create the first listener synchronously so startup can report collisions/errors.
    let first = unsafe {
        CreateNamedPipeW(
            path.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            32768,
            16384,
            1000,
            null(),
        )
    };
    if first == INVALID_HANDLE_VALUE {
        return Err(native::error("CreateNamedPipe"));
    }
    let first = first as usize;
    std::thread::spawn(move || unsafe {
        let mut pipe = first as HANDLE;
        loop {
            {
                let mut file = File::from_raw_handle(pipe);
                if (ConnectNamedPipe(pipe, std::ptr::null_mut()) != 0
                    || GetLastError() == ERROR_PIPE_CONNECTED)
                    && allowed(pipe)
                {
                    if let Ok(line) = read_line(&mut file) {
                        if let Ok(request) = serde_json::from_str::<Request>(&line) {
                            let (tx, rx) = mpsc::channel();
                            let delivery = Box::into_raw(Box::new(Delivery { request, reply: tx }));
                            if PostMessageW(hwnd as HWND, MESSAGE, 0, delivery as isize) == 0 {
                                drop(Box::from_raw(delivery));
                            } else if let Ok(reply) = rx.recv_timeout(Duration::from_secs(5)) {
                                let _ = writeln!(file, "{reply}");
                                FlushFileBuffers(pipe);
                            }
                        }
                    }
                }
                DisconnectNamedPipe(pipe);
            }
            pipe = CreateNamedPipeW(
                path.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                32768,
                16384,
                1000,
                null(),
            );
            if pipe == INVALID_HANDLE_VALUE {
                break;
            }
        }
    });
    Ok(())
}
pub fn request(request: &Request) -> Result<Value, String> {
    let path = format!("\\\\.\\pipe\\{}", name());
    unsafe {
        WaitNamedPipeW(wide(&path).as_ptr(), 1500);
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(1500);
    let mut file = loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => break file,
            Err(e)
                if std::time::Instant::now() < deadline
                    && (e.kind() == std::io::ErrorKind::NotFound
                        || e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)) =>
            {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    writeln!(file, "{}", serde_json::to_string(request).unwrap()).map_err(|e| e.to_string())?;
    serde_json::from_str(&read_line(&mut file)?).map_err(|e| e.to_string())
}
