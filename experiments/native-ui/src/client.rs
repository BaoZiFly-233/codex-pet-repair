use crate::protocol::UiState;
use std::{
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, RemoteDesktop::*, Threading::*, IO::*},
};

#[derive(Debug)]
pub enum Error {
    Connection(String),
    Action(String, Option<Box<UiState>>),
}
impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Connection(value)
    }
}
impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Connection(value.into())
    }
}

pub fn session() -> u32 {
    let mut session = 0;
    unsafe {
        ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
    }
    session
}

fn transfer(
    file: &File,
    bytes: &mut [u8],
    write: bool,
    deadline: Instant,
) -> Result<usize, String> {
    unsafe {
        let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
        if event.is_null() {
            return Err("无法准备后台通信".into());
        }
        let handle = file.as_raw_handle() as HANDLE;
        let mut overlapped = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        let mut done = 0;
        let ok = if write {
            WriteFile(
                handle,
                bytes.as_ptr(),
                bytes.len() as u32,
                std::ptr::null_mut(),
                &mut overlapped,
            )
        } else {
            ReadFile(
                handle,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                std::ptr::null_mut(),
                &mut overlapped,
            )
        };
        if ok == 0 && GetLastError() != ERROR_IO_PENDING {
            CloseHandle(event);
            return Err("后台通信失败".into());
        }
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(4000) as u32;
        if WaitForSingleObject(event, timeout) != WAIT_OBJECT_0 {
            CancelIoEx(handle, &overlapped);
            GetOverlappedResult(handle, &overlapped, &mut done, 1);
            CloseHandle(event);
            return Err("后台响应超时，请重试".into());
        }
        let result = GetOverlappedResult(handle, &overlapped, &mut done, 0);
        CloseHandle(event);
        if result == 0 {
            return Err("后台通信已中断".into());
        }
        Ok(done as usize)
    }
}

// All I/O runs on one worker; the rendering thread never waits for the pipe.
pub fn send(command: &str, enabled: bool) -> Result<UiState, Error> {
    let mut session = 0;
    unsafe {
        if ProcessIdToSessionId(GetCurrentProcessId(), &mut session) == 0 {
            return Err("无法读取会话".into());
        }
    }
    let path = format!("\\\\.\\pipe\\CodexPetRepair-v2-{session}");
    let deadline = Instant::now() + Duration::from_secs(1);
    let file = loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .open(&path)
        {
            Ok(file) => break file,
            Err(e)
                if Instant::now() < deadline
                    && (e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                        || e.kind() == std::io::ErrorKind::NotFound) =>
            {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(_) => return Err("请先启动同目录的修复后台".into()),
        }
    };
    unsafe {
        let mut pid = 0;
        if GetNamedPipeServerProcessId(file.as_raw_handle() as HANDLE, &mut pid) == 0 {
            return Err("无法核对后台身份".into());
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err("无法读取后台路径".into());
        }
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(process);
        if ok == 0 {
            return Err("无法读取后台路径".into());
        }
        let actual = String::from_utf16_lossy(&buffer[..size as usize]);
        let own = std::env::current_exe().map_err(|_| "无法读取界面路径")?;
        let expected = own
            .parent()
            .and_then(|p| p.parent())
            .ok_or("目录结构无效")?
            .join("PetRepair.exe");
        if !actual.eq_ignore_ascii_case(&expected.to_string_lossy()) {
            return Err("后台程序路径不匹配".into());
        }
    }
    let enabled = matches!(
        command,
        "automatic" | "autostart" | "tray_only" | "look_at_mouse" | "confirm"
    )
    .then_some(enabled);
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut request = format!(
        "{}\n",
        serde_json::json!({"command":command,"enabled":enabled})
    )
    .into_bytes();
    if transfer(&file, &mut request, true, deadline)? != request.len() {
        return Err("请求发送不完整".into());
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    while bytes.len() < 32768 {
        let count = transfer(&file, &mut chunk, false, deadline)?;
        if count == 0 {
            return Err("后台连接已结束".into());
        }
        let newline = chunk[..count].iter().position(|b| *b == b'\n');
        bytes.extend_from_slice(&chunk[..newline.unwrap_or(count)]);
        if bytes.len() > 32768 {
            return Err("后台响应过长".into());
        }
        if newline.is_some() {
            return decode(&bytes);
        }
    }
    Err("后台响应过长".into())
}

fn decode(bytes: &[u8]) -> Result<UiState, Error> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "后台响应格式无效")?;
    let state = value
        .get("status")
        .map(|_| serde_json::from_value::<UiState>(value.clone()))
        .transpose()
        .map_err(|_| "后台状态格式无效")?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(Error::Action(error.into(), state.map(Box::new)));
    }
    state
        .ok_or_else(|| Error::Connection("后台版本不匹配，请使用同一候选包中的主程序和界面".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timed_out_read_is_cancelled_and_the_pipe_remains_usable() {
        use std::{io::Write, os::windows::io::FromRawHandle, sync::mpsc};
        let name = format!("\\\\.\\pipe\\PetRepair-io-test-{}", std::process::id());
        let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_WAIT,
                1,
                1024,
                1024,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let mut server = unsafe { File::from_raw_handle(handle.cast()) };
        let (ready_tx, ready_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            unsafe {
                let connected =
                    ConnectNamedPipe(server.as_raw_handle().cast(), std::ptr::null_mut());
                assert!(connected != 0 || GetLastError() == ERROR_PIPE_CONNECTED);
            }
            ready_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            server.write_all(b"ok\n").unwrap();
            unsafe {
                FlushFileBuffers(server.as_raw_handle().cast());
            }
        });
        let client = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .open(name)
            .unwrap();
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut bytes = [0u8; 16];
        let started = Instant::now();
        let result = transfer(
            &client,
            &mut bytes,
            false,
            started + Duration::from_millis(50),
        );
        assert!(result.unwrap_err().contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(2));
        finish_tx.send(()).unwrap();
        let count = transfer(
            &client,
            &mut bytes,
            false,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(&bytes[..count], b"ok\n");
        worker.join().unwrap();
    }
    #[test]
    fn action_errors_preserve_authoritative_settings() {
        let result = decode(
            br#"{"status":"failed","automatic":false,"look_at_mouse":true,"error":"save failed"}"#,
        );
        assert!(matches!(
            result,
            Err(Error::Action(_, Some(state))) if !state.automatic && state.look_at_mouse
        ));
        assert!(matches!(
            decode(br#"{"error":"clipboard busy"}"#),
            Err(Error::Action(_, None))
        ));
        assert!(matches!(decode(b"broken"), Err(Error::Connection(_))));
    }
}
