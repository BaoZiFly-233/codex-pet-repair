#![cfg_attr(not(test), windows_subsystem = "windows")]
mod bridge;
mod native;
mod policy;
#[path = "../shared/protocol.rs"]
mod protocol;
mod repair;
mod selftest;
#[path = "../shared/signal.rs"]
mod signal;
mod storage;
mod ui;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|s| s == "--control") {
        let result = args
            .get(i + 1)
            .ok_or_else(|| "missing_request".to_string())
            .and_then(|s| serde_json::from_str::<bridge::Request>(s).map_err(|e| e.to_string()))
            .and_then(|r| bridge::request(&r));
        let value = result.unwrap_or_else(|e| serde_json::json!({"error":e}));
        if let Some(path) = args.get(i + 2) {
            let _ = storage::write_json(std::path::Path::new(path), &value);
        }
        return;
    }
    if args.iter().any(|s| s == "--repair-worker") {
        repair::worker();
        return;
    }
    if let Some(i) = args.iter().position(|s| s == "--test-orphan-parent") {
        if let Some(s) = args.get(i + 1) {
            selftest::orphan_parent(s);
        }
        return;
    }
    if let Some(i) = args.iter().position(|s| s == "--probe") {
        let mut d = native::Discovery::default();
        d.refresh_processes();
        if let Some(p) = args.get(i + 1) {
            let _ = storage::write_json(std::path::Path::new(p), &d.scan());
        }
        return;
    }
    if let Some(i) = args.iter().position(|s| s == "--self-test") {
        if let Some(p) = args.get(i + 1) {
            selftest::run(std::path::Path::new(p));
        }
        return;
    }
    if let Some(i) = args.iter().position(|s| s == "--repair-once") {
        if let Some(p) = args.get(i + 1) {
            selftest::real_once(std::path::Path::new(p));
        }
        return;
    }
    ui::run(
        args.iter().any(|s| s == "--tray"),
        args.iter().any(|s| s == "--ui"),
    );
}
