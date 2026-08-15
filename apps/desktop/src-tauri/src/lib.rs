use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

const DEFAULT_PORT: u16 = 3080;
const PORT_SCAN_LIMIT: u16 = 100;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshStatus {
    pub state: String,
    pub port: Option<u16>,
    pub error: Option<String>,
}

impl DshStatus {
    fn stopped() -> Self {
        Self {
            state: "stopped".into(),
            port: None,
            error: None,
        }
    }

    fn starting() -> Self {
        Self {
            state: "starting".into(),
            port: None,
            error: None,
        }
    }

    fn ready(port: u16) -> Self {
        Self {
            state: "ready".into(),
            port: Some(port),
            error: None,
        }
    }

    fn failed(error: impl Into<String>) -> Self {
        Self {
            state: "failed".into(),
            port: None,
            error: Some(error.into()),
        }
    }
}

struct RuntimeState {
    child: Option<Child>,
    status: DshStatus,
}

pub struct DshManager {
    runtime: Mutex<RuntimeState>,
}

impl Default for DshManager {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(RuntimeState {
                child: None,
                status: DshStatus::stopped(),
            }),
        }
    }
}

impl DshManager {
    fn status(&self) -> DshStatus {
        self.runtime
            .lock()
            .expect("dsh manager mutex poisoned")
            .status
            .clone()
    }

    fn set_status(&self, status: DshStatus) {
        self.runtime
            .lock()
            .expect("dsh manager mutex poisoned")
            .status = status;
    }

    fn take_child(&self) -> Option<Child> {
        self.runtime
            .lock()
            .expect("dsh manager mutex poisoned")
            .child
            .take()
    }

    fn set_child(&self, child: Child) {
        self.runtime
            .lock()
            .expect("dsh manager mutex poisoned")
            .child = Some(child);
    }
}

#[tauri::command]
fn get_dsh_status(manager: State<'_, DshManager>) -> DshStatus {
    manager.status()
}

#[tauri::command]
fn start_dsh(app: AppHandle, manager: State<'_, DshManager>) -> Result<DshStatus, String> {
    start_internal(&app, manager.inner())
}

#[tauri::command]
fn stop_dsh(app: AppHandle, manager: State<'_, DshManager>) -> Result<DshStatus, String> {
    stop_internal(&app, manager.inner())
}

#[tauri::command]
fn restart_dsh(app: AppHandle, manager: State<'_, DshManager>) -> Result<DshStatus, String> {
    stop_internal(&app, manager.inner())?;
    start_internal(&app, manager.inner())
}

#[tauri::command]
fn select_workspace(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .set_title("选择 Harness workspace")
        .blocking_pick_folder()
        .map(|path| path.to_string()))
}

#[tauri::command]
fn open_app_data_dir(app: AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    open_path(&path)?;
    Ok(path.to_string_lossy().into_owned())
}

fn start_internal(app: &AppHandle, manager: &DshManager) -> Result<DshStatus, String> {
    let current = manager.status();
    if current.state == "ready" {
        return Ok(current);
    }

    stop_internal(app, manager)?;
    publish_status(app, manager, DshStatus::starting())?;

    match launch_dsh(app, manager) {
        Ok(status) => Ok(status),
        Err(error) => publish_status(app, manager, DshStatus::failed(error)),
    }
}

fn launch_dsh(app: &AppHandle, manager: &DshManager) -> Result<DshStatus, String> {
    let port = find_available_port()?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;

    let runtime = resolve_runtime(app)?;
    let node = resolve_node(&runtime)?;
    let entrypoint = resolve_entrypoint(&runtime)?;
    let log_path = data_dir.join("logs").join("dsh.log");
    fs::create_dir_all(log_path.parent().expect("dsh log path has a parent"))
        .map_err(|error| error.to_string())?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            format!(
                "failed to open dsh startup log {}: {error}",
                log_path.display()
            )
        })?;
    let working_dir = entrypoint
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "dsh entrypoint has no application directory".to_string())?;

    let mut command = Command::new(node);
    command
        .arg(&entrypoint)
        .args(["web", "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .current_dir(working_dir)
        .env("DSH_HOME", &data_dir)
        .env("DSH_DESKTOP", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file.try_clone().map_err(|error| {
            format!("failed to prepare dsh stdout log: {error}")
        })?))
        .stderr(Stdio::from(log_file));
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| error.to_string())?;

    if !wait_for_server(&mut child, port) {
        let error = startup_failure(&mut child, &log_path);
        terminate_child(&mut child);
        return Err(error);
    }

    manager.set_child(child);
    publish_status(app, manager, DshStatus::ready(port))
}

fn startup_failure(child: &mut Child, log_path: &Path) -> String {
    let mut error = if let Ok(Some(exit)) = child.try_wait() {
        format!("dsh exited before becoming ready ({exit})")
    } else {
        "dsh did not become ready before the startup timeout".to_string()
    };
    error.push_str(&format!(". Startup log: {}", log_path.display()));
    if let Some(log_tail) = read_log_tail(log_path) {
        error.push_str("\n");
        error.push_str(&log_tail);
    }
    error
}

fn read_log_tail(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let mut lines: Vec<&str> = contents.lines().rev().take(12).collect();
    lines.reverse();
    let tail = lines.join("\n");
    (!tail.is_empty()).then_some(tail)
}

fn stop_internal(app: &AppHandle, manager: &DshManager) -> Result<DshStatus, String> {
    if let Some(mut child) = manager.take_child() {
        terminate_child(&mut child);
    }
    publish_status(app, manager, DshStatus::stopped())
}

fn publish_status(
    app: &AppHandle,
    manager: &DshManager,
    status: DshStatus,
) -> Result<DshStatus, String> {
    manager.set_status(status.clone());
    app.emit("dsh-status", status.clone())
        .map_err(|error| error.to_string())?;
    Ok(status)
}

fn find_available_port() -> Result<u16, String> {
    find_available_port_from(DEFAULT_PORT, PORT_SCAN_LIMIT)
}

fn find_available_port_from(start: u16, limit: u16) -> Result<u16, String> {
    (0..limit)
        .map(|offset| start.saturating_add(offset))
        .find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .ok_or_else(|| {
            format!(
                "no available localhost port in {start}..{}",
                start.saturating_add(limit.saturating_sub(1))
            )
        })
}

fn wait_for_server(child: &mut Child, port: u16) -> bool {
    wait_until_ready(
        STARTUP_TIMEOUT,
        POLL_INTERVAL,
        || matches!(child.try_wait(), Ok(Some(_))),
        || is_http_ready(port),
    )
}

fn wait_until_ready(
    timeout: Duration,
    poll_interval: Duration,
    mut stopped: impl FnMut() -> bool,
    mut ready: impl FnMut() -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if stopped() {
            return false;
        }
        if ready() {
            return true;
        }
        thread::sleep(poll_interval);
    }
    false
}

fn is_http_ready(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    if stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = [0_u8; 128];
    let Ok(size) = stream.read(&mut response) else {
        return false;
    };
    let response = String::from_utf8_lossy(&response[..size]);
    response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.1 3")
}

fn resolve_runtime(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(root) = std::env::var("DSH_DESKTOP_RUNTIME_ROOT") {
        return Ok(PathBuf::from(root));
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled_root = resource_dir.join("runtime");
        let bundled_target = bundled_root.join(runtime_target());
        if bundled_target.exists() {
            return Ok(bundled_target);
        }
        if bundled_root.exists() {
            return Ok(bundled_root);
        }
    }

    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    if checkout.join("apps/cli").exists() {
        return Ok(checkout);
    }

    Err("DeepSeek Harness runtime was not found".to_string())
}

fn resolve_node(runtime: &Path) -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("DSH_NODE_BIN") {
        return Ok(PathBuf::from(path));
    }

    let candidates = [
        runtime.join("node").join(node_filename()),
        runtime
            .join("node")
            .join(runtime_target())
            .join(node_filename()),
    ];
    if let Some(bundled) = candidates.into_iter().find(|path| path.exists()) {
        return Ok(bundled);
    }

    if cfg!(debug_assertions) {
        return Ok(PathBuf::from("node"));
    }

    Err(
        "bundled Node.js runtime was not found; run prepare-desktop-runtime before packaging"
            .to_string(),
    )
}

fn resolve_entrypoint(runtime: &Path) -> Result<PathBuf, String> {
    let candidates = [
        runtime.join("apps/cli/lib/bin.js"),
        runtime.join("dsh/lib/bin.js"),
        runtime.join("dsh/apps/cli/lib/bin.js"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "built dsh entrypoint was not found; run pnpm run build first".to_string())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn runtime_target() -> &'static str {
    "darwin-arm64"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn runtime_target() -> &'static str {
    "darwin-x64"
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn runtime_target() -> &'static str {
    "windows-x64"
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
fn runtime_target() -> &'static str {
    "unsupported"
}

fn node_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "node.exe"
    } else {
        "node"
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(target_os = "windows")]
    {
        if !matches!(child.try_wait(), Ok(Some(_))) {
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = child.kill();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let process_group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-TERM", &process_group])
            .status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = Command::new("kill")
            .args(["-KILL", &process_group])
            .status();
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        command.arg(path);
        command
            .spawn()
            .map_err(|error| error.to_string())
            .map(|_| ())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err("opening the app data directory is not supported on this platform".to_string())
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DshManager::default())
        .invoke_handler(tauri::generate_handler![
            start_dsh,
            stop_dsh,
            restart_dsh,
            get_dsh_status,
            select_workspace,
            open_app_data_dir
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::CloseRequested { .. }) {
                        let manager = app_handle.state::<DshManager>();
                        let _ = stop_internal(&app_handle, manager.inner());
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DeepSeek Harness Desktop");
}

#[cfg(test)]
mod tests {
    use super::{
        find_available_port_from, is_http_ready, node_filename, runtime_target, wait_until_ready,
    };
    use std::{net::TcpListener, thread, time::Duration};

    #[test]
    fn detects_an_http_server() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener address").port();
        let worker = thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 256];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
            );
        });
        assert!(is_http_ready(port));
        worker.join().expect("test server join");
    }

    #[test]
    fn uses_platform_runtime_names() {
        assert!(!runtime_target().is_empty());
        assert!(node_filename() == "node" || node_filename() == "node.exe");
    }

    #[test]
    fn rejects_an_occupied_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind occupied port");
        let port = listener.local_addr().expect("listener address").port();
        assert!(find_available_port_from(port, 1).is_err());
    }

    #[test]
    fn times_out_when_server_never_becomes_ready() {
        assert!(!wait_until_ready(
            Duration::from_millis(5),
            Duration::from_millis(1),
            || false,
            || false,
        ));
    }

    #[test]
    fn stops_waiting_after_early_process_exit() {
        assert!(!wait_until_ready(
            Duration::from_secs(1),
            Duration::from_millis(1),
            || true,
            || false,
        ));
    }
}
