use windows_sys::Win32::{Foundation::*, System::Threading::*};

/// Auto-reset kernel events coalesce notifications. State is always read through authenticated IPC.
pub struct Signal(HANDLE);
unsafe impl Send for Signal {}
unsafe impl Sync for Signal {}
impl Signal {
    pub fn new(name: Option<&str>) -> Result<Self, String> {
        let name = name.map(|s| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
        let handle = unsafe {
            CreateEventW(
                std::ptr::null(),
                0,
                0,
                name.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            )
        };
        if handle.is_null() {
            Err("无法创建状态通知事件".into())
        } else {
            Ok(Self(handle))
        }
    }
    pub fn set(&self) {
        unsafe {
            SetEvent(self.0);
        }
    }
    #[allow(dead_code)] // Only the UI waits; the core publishes through the same shared type.
    pub fn wait(signals: &[&Self], timeout_ms: u32) -> Result<(), String> {
        let handles: Vec<_> = signals.iter().map(|s| s.0).collect();
        let result = unsafe {
            WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, timeout_ms)
        };
        if result == WAIT_FAILED {
            Err("无法等待后台状态通知".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Signal {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_coalesce_and_reset() {
        let signal = Signal::new(None).unwrap();
        signal.set();
        signal.set();
        assert_eq!(unsafe { WaitForSingleObject(signal.0, 0) }, WAIT_OBJECT_0);
        assert_eq!(unsafe { WaitForSingleObject(signal.0, 0) }, WAIT_TIMEOUT);
    }
}
