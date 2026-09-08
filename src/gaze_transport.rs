//! Local CDP transport with bounded HTTP and WebSocket I/O, without an async runtime.
use crate::native::wide;
use serde_json::{json, Value};
use std::{
    ffi::c_void,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

pub const PORT: u16 = 19227;
const LIMIT: usize = 512 * 1024;
struct Internet(*mut c_void);
impl Internet {
    fn new(handle: *mut c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err(format!("WinHTTP:{}", unsafe { GetLastError() }))
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn check(ok: i32) -> Result<(), String> {
    if ok == 0 {
        Err(format!("WinHTTP:{}", unsafe { GetLastError() }))
    } else {
        Ok(())
    }
}
pub struct Local {
    connection: Internet,
    _session: Internet,
}
impl Local {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let session = Internet::new(WinHttpOpen(
                wide("PetRepair/1").as_ptr(),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                null(),
                null(),
                0,
            ))?;
            check(WinHttpSetTimeouts(session.0, 1200, 1200, 1200, 1200))?;
            let connection = Internet::new(WinHttpConnect(
                session.0,
                wide("127.0.0.1").as_ptr(),
                PORT,
                0,
            ))?;
            Ok(Self {
                connection,
                _session: session,
            })
        }
    }
    fn request(&self, path: &str) -> Result<Internet, String> {
        unsafe {
            let request = Internet::new(WinHttpOpenRequest(
                self.connection.0,
                wide("GET").as_ptr(),
                wide(path).as_ptr(),
                null(),
                null(),
                null(),
                0,
            ))?;
            let disabled = WINHTTP_DISABLE_REDIRECTS
                | WINHTTP_DISABLE_COOKIES
                | WINHTTP_DISABLE_AUTHENTICATION;
            check(WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_DISABLE_FEATURE,
                &disabled as *const _ as _,
                4,
            ))?;
            check(WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0))?;
            check(WinHttpReceiveResponse(request.0, null_mut()))?;
            let mut status = 0u32;
            let mut size = 4;
            check(WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                &mut status as *mut _ as _,
                &mut size,
                null_mut(),
            ))?;
            if status != 200 {
                return Err(format!("HTTP:{status}"));
            }
            Ok(request)
        }
    }
    pub fn targets(&self) -> Result<Vec<Value>, String> {
        unsafe {
            let request = self.request("/json/list")?;
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut body = Vec::new();
            loop {
                let mut buffer = [0u8; 8192];
                let mut count = 0;
                check(WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr() as _,
                    buffer.len() as u32,
                    &mut count,
                ))?;
                if count == 0 {
                    break;
                }
                body.extend_from_slice(&buffer[..count as usize]);
                if body.len() > LIMIT || Instant::now() > deadline {
                    return Err("target_list_limit".into());
                }
            }
            serde_json::from_slice(&body).map_err(|_| "target_list_invalid".into())
        }
    }
    pub fn attach(&self, url: &str) -> Result<Socket, String> {
        websocket_path(url).ok_or("websocket_address_rejected")?;
        let stream = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)),
            Duration::from_millis(1200),
        )
        .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(1200)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(1200)))
            .map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        let config = tungstenite::protocol::WebSocketConfig::default()
            .read_buffer_size(8192)
            .write_buffer_size(0)
            .max_message_size(Some(LIMIT))
            .max_frame_size(Some(64 * 1024));
        let (handle, _) = tungstenite::client::client_with_config(url, stream, Some(config))
            .map_err(|_| "websocket_handshake_failed")?;
        Ok(Socket {
            handle,
            sequence: 0,
        })
    }
}
pub fn websocket_path(url: &str) -> Option<&str> {
    let prefix = format!("ws://127.0.0.1:{PORT}/devtools/page/");
    let id = url.strip_prefix(&prefix)?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    url.find("/devtools/").map(|i| &url[i..])
}
pub fn overlay_url(url: &str) -> bool {
    if !(url.starts_with("app://") || url.starts_with("file://")) {
        return false;
    }
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let route: Vec<_> = query
        .split('&')
        .filter_map(|p| p.strip_prefix("initialRoute="))
        .collect();
    if !route.is_empty() {
        return route.len() == 1
            && matches!(
                route[0],
                "/avatar-overlay" | "%2Favatar-overlay" | "%2favatar-overlay"
            );
    }
    base.strip_prefix("app://")
        .and_then(|p| p.find('/').map(|i| &p[i..]))
        == Some("/avatar-overlay")
}
pub struct Socket {
    handle: tungstenite::WebSocket<TcpStream>,
    sequence: u64,
}
impl Socket {
    pub fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        self.sequence += 1;
        let text = json!({"id":self.sequence,"method":"Runtime.evaluate","params":{"expression":expression,"awaitPromise":true,"returnByValue":true,"timeout":1800}}).to_string();
        self.handle
            .send(tungstenite::Message::Text(text.into()))
            .map_err(|_| "websocket_send_failed")?;
        let deadline = Instant::now() + Duration::from_millis(2200);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("websocket_response_timeout".into());
            }
            self.handle
                .get_mut()
                .set_read_timeout(Some(remaining))
                .map_err(|e| e.to_string())?;
            let message = self.handle.read().map_err(|_| "websocket_receive_failed")?;
            let text = match message {
                tungstenite::Message::Text(text) => text,
                tungstenite::Message::Ping(_) | tungstenite::Message::Pong(_) => continue,
                _ => return Err("websocket_closed_or_binary".into()),
            };
            let value: Value = serde_json::from_str(&text).map_err(|_| "websocket_invalid_json")?;
            if value["id"].as_u64() != Some(self.sequence) {
                continue;
            }
            if value.get("error").is_some() || value["result"].get("exceptionDetails").is_some() {
                return Err("renderer_evaluation_failed".into());
            }
            return Ok(value["result"]["result"]["value"].clone());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn socket_pair(
        action: impl FnOnce(tungstenite::WebSocket<TcpStream>) + Send + 'static,
    ) -> (Socket, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            action(tungstenite::accept(stream).unwrap());
        });
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(1200)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_millis(1200)))
            .unwrap();
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(LIMIT))
            .max_frame_size(Some(64 * 1024));
        let (handle, _) = tungstenite::client::client_with_config(
            format!("ws://{address}/test"),
            stream,
            Some(config),
        )
        .unwrap();
        (
            Socket {
                handle,
                sequence: 0,
            },
            server,
        )
    }
    #[test]
    fn websocket_ignores_events_and_matches_reply_id() {
        let (mut socket, server) = socket_pair(|mut server| {
            let request: Value =
                serde_json::from_str(server.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["method"], "Runtime.evaluate");
            for value in [
                json!({"method":"Page.event"}),
                json!({"id":99}),
                json!({"id":1,"result":{"result":{"value":{"code":"ready"}}}}),
            ] {
                server
                    .send(tungstenite::Message::Text(value.to_string().into()))
                    .unwrap();
            }
        });
        assert_eq!(socket.evaluate("test").unwrap()["code"], "ready");
        server.join().unwrap();
    }
    #[test]
    fn websocket_rejects_exception_and_oversized_frame() {
        for value in [
            json!({"id":1,"result":{"exceptionDetails":{"text":"failure"}}}).to_string(),
            "x".repeat(LIMIT + 1),
        ] {
            let (mut socket, server) = socket_pair(move |mut server| {
                let _ = server.read();
                let _ = server.send(tungstenite::Message::Text(value.into()));
            });
            assert!(socket.evaluate("test").is_err());
            server.join().unwrap();
        }
    }
    #[test]
    fn websocket_silent_peer_has_a_deadline() {
        let (mut socket, server) = socket_pair(|mut server| {
            let _ = server.read();
            std::thread::sleep(Duration::from_millis(2600));
        });
        let start = Instant::now();
        assert!(socket.evaluate("test").is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        server.join().unwrap();
    }
    #[test]
    fn endpoints_are_exact_and_local() {
        assert_eq!(
            websocket_path("ws://127.0.0.1:19227/devtools/page/ABC-123"),
            Some("/devtools/page/ABC-123")
        );
        for url in [
            "ws://localhost:19227/devtools/page/a",
            "ws://127.0.0.1:9222/devtools/page/a",
            "ws://127.0.0.1:19227/devtools/page/../browser/a",
            "ws://127.0.0.1:19227/devtools/page/a?x",
            "ws://127.0.0.1:19227/devtools/page/",
        ] {
            assert!(websocket_path(url).is_none());
        }
    }
    #[test]
    fn routes_do_not_match_by_title_or_substring() {
        assert!(overlay_url(
            "app://-/index.html?initialRoute=%2Favatar-overlay"
        ));
        assert!(overlay_url("app://-/avatar-overlay"));
        for url in [
            "https://example.org/avatar-overlay",
            "app://-/avatar-overlay-fake",
            "app://-/index.html?title=avatar-overlay",
            "app://-/index.html?initialRoute=/avatar-overlay&initialRoute=/chat",
        ] {
            assert!(!overlay_url(url));
        }
    }
}
