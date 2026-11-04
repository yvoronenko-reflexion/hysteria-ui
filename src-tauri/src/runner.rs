//! Runs and supervises the `hysteria` client process.
//!
//! Modes:
//! * **proxy** — a normal child process (no privileges).
//! * **tun** — system-wide routing needs root. Two paths:
//!   - if the one-time sudoers rule is installed, launch via `sudo -n` (no
//!     prompt) and stream logs live like proxy mode;
//!   - otherwise fall back to `osascript ... with administrator privileges`,
//!     which prompts on every connect/disconnect.
//!
//! One connection is active at a time. Status is stored (queryable via the
//! `status` command) and pushed to the frontend via the `connection-status`
//! event; log lines are pushed via `hysteria-log`.

use crate::config;
use crate::model::{HysteriaConfig, Profile};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const STATUS_EVENT: &str = "connection-status";
const LOG_EVENT: &str = "hysteria-log";
const TRAFFIC_EVENT: &str = "traffic-stats";
const SUDOERS_FILE: &str = "/etc/sudoers.d/hysteria-ui";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    /// disconnected | connecting | connected | error
    pub state: String,
    pub mode: Option<String>,
    pub message: Option<String>,
    /// Which profile this connection belongs to (id + display name).
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    /// The `count` field from hysteria's "connected to server" line: how many
    /// times the QUIC connection has been (re)established. 2+ means the
    /// connection dropped and was rebuilt at least once.
    pub connect_count: Option<u64>,
    /// Epoch millis of when the session first reached "connected" (kept across
    /// silent redials so the UI shows session uptime, not last-dial uptime).
    pub connected_at: Option<u64>,
}

impl StatusPayload {
    fn disconnected() -> Self {
        Self {
            state: "disconnected".into(),
            mode: None,
            message: None,
            profile_id: None,
            profile_name: None,
            connect_count: None,
            connected_at: None,
        }
    }

    fn error() -> Self {
        Self { state: "error".into(), ..Self::disconnected() }
    }
}

/// Session traffic counters, pushed once a second while connected.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficPayload {
    /// Bytes received / sent since the connection started.
    pub rx: u64,
    pub tx: u64,
    /// Current rates in bytes per second.
    pub rx_rate: f64,
    pub tx_rate: f64,
}

/// Where the traffic monitor reads byte counters from.
enum TrafficSource {
    /// TUN mode: the utun interface counters (netstat).
    Interface(String),
    /// Proxy mode: per-process counters for the hysteria child (nettop).
    Process(u32),
}

/// How to stop the running child.
enum StopKind {
    /// We own the process — signal it directly.
    Kill,
    /// The process runs as root (via sudo); stop it with a passwordless pkill
    /// matched on its config path.
    SudoPkill(PathBuf),
}

enum Active {
    None,
    Child { child: Child, stop: StopKind },
    /// TUN launched via osascript; supervised by tailing a logfile + pidfile.
    TunOsascript { pidfile: PathBuf },
}

pub struct RunnerState {
    active: Mutex<Active>,
    status: Mutex<StatusPayload>,
    /// Signals the osascript-tun watcher thread to stop.
    stop: Mutex<Option<Arc<AtomicBool>>>,
    /// Signals the traffic monitor thread to stop.
    traffic_stop: Mutex<Option<Arc<AtomicBool>>>,
    /// Last FATAL-ish line seen; used as the error message when the process
    /// dies before the connection was established.
    last_error: Mutex<Option<String>>,
    /// Most recent (profile, mode) handed to `connect`, post-`apply_mode`.
    /// Lets the tray reconnect and the IP lookup find the proxy listener.
    last_conn: Mutex<Option<(Profile, String)>>,
}

impl Default for RunnerState {
    fn default() -> Self {
        Self {
            active: Mutex::new(Active::None),
            status: Mutex::new(StatusPayload::disconnected()),
            stop: Mutex::new(None),
            traffic_stop: Mutex::new(None),
            last_error: Mutex::new(None),
            last_conn: Mutex::new(None),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn set_status(app: &AppHandle, payload: StatusPayload) {
    let state = app.state::<RunnerState>();
    *state.status.lock().unwrap() = payload.clone();
    crate::tray::update(app, &payload);
    let _ = app.emit(STATUS_EVENT, payload);
}

fn emit_log(app: &AppHandle, line: &str) {
    let _ = app.emit(LOG_EVENT, strip_ansi(line));
}

/// What a hysteria output line tells us about the connection.
#[derive(Debug, PartialEq)]
enum LineEvent {
    /// The QUIC handshake succeeded ("connected to server").
    Connected,
    /// A fatal startup error; the process is about to exit.
    Fatal,
    None,
}

fn classify_line(line: &str) -> LineEvent {
    if line.contains("connected to server") {
        return LineEvent::Connected;
    }
    // zap's console encoder prints the level in caps; FATAL only occurs for
    // unrecoverable startup errors (bad config, handshake/auth failure).
    if line.contains("FATAL")
        || line.contains("failed to initialize client")
        || line.contains("failed to load client config")
        || line.contains("failed to parse client config")
    {
        return LineEvent::Fatal;
    }
    LineEvent::None
}

/// Extract the `count` field from a "connected to server" line, e.g.
/// `... INFO connected to server {"addr": ..., "count": 3}`.
fn parse_connect_count(line: &str) -> Option<u64> {
    let rest = &line[line.find("\"count\":")? + "\"count\":".len()..];
    let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Trim the timestamp prefix off a fatal log line for the status message.
fn error_message(line: &str) -> String {
    match line.find("FATAL") {
        Some(i) => line[i..].trim().to_string(),
        None => line.trim().to_string(),
    }
}

/// Forward one line of hysteria output to the UI and update the connection
/// state it implies: `connecting` becomes `connected` only once the handshake
/// is confirmed, and fatal lines flip it to `error`.
fn process_child_line(app: &AppHandle, raw: &str) {
    let line = strip_ansi(raw);
    let _ = app.emit(LOG_EVENT, line.clone());

    let state = app.state::<RunnerState>();
    match classify_line(&line) {
        LineEvent::Connected => {
            let mut cur = state.status.lock().unwrap().clone();
            // "connecting" → the initial handshake; "connected" → hysteria
            // silently redialed after a dropped connection (or a lazy-mode
            // client completed its deferred first handshake) and logged the
            // line again with an incremented count.
            if cur.state == "connecting" || cur.state == "connected" {
                cur.state = "connected".into();
                cur.message = None;
                cur.connect_count = parse_connect_count(&line).or(cur.connect_count);
                cur.connected_at = cur.connected_at.or_else(|| Some(now_ms()));
                set_status(app, cur);
            }
        }
        LineEvent::Fatal => {
            let msg = error_message(&line);
            *state.last_error.lock().unwrap() = Some(msg.clone());
            let mut cur = state.status.lock().unwrap().clone();
            if cur.state == "connecting" {
                cur.state = "error".into();
                cur.mode = None;
                cur.message = Some(msg);
                set_status(app, cur);
            }
        }
        LineEvent::None => {}
    }
}

/// The hysteria process ended on its own (not via disconnect). Preserve an
/// error state if we have one; a death while still connecting is an error too.
fn handle_unexpected_exit(app: &AppHandle) {
    stop_traffic_monitor(app);
    emit_log(app, "hysteria exited.");
    let state = app.state::<RunnerState>();
    let cur = state.status.lock().unwrap().state.clone();
    match cur.as_str() {
        "connecting" => {
            let msg = state
                .last_error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "hysteria exited before the connection was established".into());
            set_status(
                app,
                StatusPayload { message: Some(msg), ..StatusPayload::error() },
            );
        }
        // A fatal line already set the error status — keep its message.
        "error" => {}
        _ => set_status(app, StatusPayload::disconnected()),
    }
}

/// Remove ANSI escape sequences (colors, cursor movement, OSC titles) so the
/// UI log pane shows plain text.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI: ESC [ <params/intermediates> <final byte 0x40-0x7e>
            Some('[') => {
                chars.next();
                for n in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&n) {
                        break;
                    }
                }
            }
            // OSC: ESC ] ... terminated by BEL or ESC \
            Some(']') => {
                chars.next();
                while let Some(n) = chars.next() {
                    if n == '\x07' {
                        break;
                    }
                    if n == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            // Two-character escape (ESC c, ESC 7, ...)
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// Locate the hysteria binary: bundled resource (prod) or repo `binaries/` (dev).
fn hysteria_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(p) = app
        .path()
        .resolve("binaries/hysteria", tauri::path::BaseDirectory::Resource)
    {
        if p.exists() {
            return Ok(p);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("hysteria");
    if dev.exists() {
        return Ok(dev);
    }
    Err("hysteria binary not found. Expected it at src-tauri/binaries/hysteria.".to_string())
}

/// Which hysteria binary the app runs, reported to the UI.
#[derive(Clone, serde::Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub path: String,
}

/// `hysteria version` for the binary this app would launch. Cheap enough to run
/// on demand — the UI asks once at startup.
pub fn hysteria_version(app: &AppHandle) -> Result<VersionInfo, String> {
    let bin = hysteria_path(app)?;
    let out = Command::new(&bin)
        .arg("version")
        .output()
        .map_err(|e| format!("Failed to run hysteria: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = parse_version(&text)
        .ok_or_else(|| "Could not read a version from `hysteria version`.".to_string())?;
    Ok(VersionInfo {
        version,
        path: bin.to_string_lossy().to_string(),
    })
}

/// Pull the version out of `hysteria version` output. The command prints an
/// ASCII-art banner first, then `Key:\tValue` lines — only `Version:` matters.
fn parse_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("Version:"))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// Runtime dir for the active config + tun logs. Uses the cache dir because its
/// path has no spaces (unlike "Application Support"), which matters for the
/// sudoers command matching.
fn runtime_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn active_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(runtime_dir(app)?.join("active-config.yaml"))
}

fn write_active_config(app: &AppHandle, yaml: &str) -> Result<PathBuf, String> {
    let path = active_config_path(app)?;
    fs::write(&path, yaml).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Derive the mode-dependent parts of a config. The mode buttons are the single
/// switch between proxy and tun — the config's own `tun.enabled` flag is
/// overridden here so the two can't disagree.
///
/// `connect` and the YAML preview both go through this, so what the user sees in
/// "View YAML" is exactly what gets written to disk at connect time.
pub fn apply_mode(cfg: &mut HysteriaConfig, mode: &str) -> Result<(), String> {
    match mode {
        "tun" => {
            cfg.tun.enabled = true;
            prepare_tun(cfg);
        }
        "proxy" => {
            cfg.tun.enabled = false;
            // hysteria needs at least one mode block or it exits with
            // "no mode specified"; fall back to the default SOCKS5 listener.
            if !cfg.socks5.enabled && !cfg.http.enabled {
                cfg.socks5.enabled = true;
                if cfg.socks5.listen.is_empty() {
                    cfg.socks5.listen = "127.0.0.1:1080".to_string();
                }
            }
        }
        other => return Err(format!("Unknown mode: {other}")),
    }
    Ok(())
}

/// Start a connection. `mode` is "proxy" or "tun".
pub fn connect(app: &AppHandle, mut profile: Profile, mode: &str) -> Result<(), String> {
    {
        let state = app.state::<RunnerState>();
        if !matches!(&*state.active.lock().unwrap(), Active::None) {
            return Err("Already connected. Disconnect first.".to_string());
        }
    }

    let bin = hysteria_path(app)?;
    let had_listener = profile.config.socks5.enabled || profile.config.http.enabled;
    apply_mode(&mut profile.config, mode)?;
    if mode == "proxy" && !had_listener {
        let listen = &profile.config.socks5.listen;
        emit_log(
            app,
            &format!("No local proxy listener enabled; starting SOCKS5 on {listen}."),
        );
    }
    let yaml = config::generate_yaml(&profile.config)?;
    let cfg_path = write_active_config(app, &yaml)?;

    {
        let state = app.state::<RunnerState>();
        *state.last_error.lock().unwrap() = None;
        *state.last_conn.lock().unwrap() = Some((profile.clone(), mode.to_string()));
    }
    set_status(
        app,
        StatusPayload {
            state: "connecting".into(),
            mode: Some(mode.into()),
            message: None,
            profile_id: Some(profile.id.clone()),
            profile_name: Some(profile.name.clone()),
            connect_count: None,
            connected_at: None,
        },
    );
    emit_log(app, &format!("Starting hysteria ({mode} mode)…"));

    // Stay in "connecting" until the log stream confirms the handshake
    // ("connected to server"); each variant also yields a traffic source.
    let result = match mode {
        "proxy" => connect_proxy(app, &bin, &cfg_path).map(TrafficSource::Process),
        "tun" => connect_tun(app, &bin, &cfg_path)
            .map(|_| TrafficSource::Interface(profile.config.tun.name.clone())),
        other => Err(format!("Unknown mode: {other}")),
    };

    match result {
        Ok(source) => {
            spawn_traffic_monitor(app, source);
            if profile.config.lazy {
                // Lazy mode defers the handshake until first traffic, so there
                // is no startup confirmation to wait for.
                set_status(
                    app,
                    StatusPayload {
                        state: "connected".into(),
                        mode: Some(mode.into()),
                        message: Some("lazy: connects on first traffic".into()),
                        profile_id: Some(profile.id.clone()),
                        profile_name: Some(profile.name.clone()),
                        connect_count: None,
                        connected_at: Some(now_ms()),
                    },
                );
            }
            Ok(())
        }
        Err(e) => {
            set_status(
                app,
                StatusPayload { message: Some(e.clone()), ..StatusPayload::error() },
            );
            Err(e)
        }
    }
}

/// Spawn a child, stream its stdout+stderr to the UI (updating connection
/// state from what the lines say), register it as active, and settle the
/// status when it exits (stdout EOF).
fn spawn_streaming(app: &AppHandle, mut child: Child, stop: StopKind) {
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                process_child_line(&app, &line);
            }
        });
    }

    let stdout = child.stdout.take();
    {
        let state = app.state::<RunnerState>();
        *state.active.lock().unwrap() = Active::Child { child, stop };
    }

    if let Some(out) = stdout {
        let app = app.clone();
        thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                process_child_line(&app, &line);
            }
            // stdout closed => process exited. Reap it and reset state.
            let state = app.state::<RunnerState>();
            let mut active = state.active.lock().unwrap();
            let was_running = matches!(&*active, Active::Child { .. });
            if let Active::Child { child, .. } = &mut *active {
                let _ = child.wait();
            }
            *active = Active::None;
            drop(active);
            // was_running is false when disconnect() already took ownership;
            // reaching here with it true means hysteria exited on its own.
            if was_running {
                handle_unexpected_exit(&app);
            }
        });
    }
}

/// Returns the child's pid for the per-process traffic monitor.
fn connect_proxy(app: &AppHandle, bin: &Path, cfg: &Path) -> Result<u32, String> {
    let child = Command::new(bin)
        .arg("client")
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start hysteria: {e}"))?;
    let pid = child.id();
    spawn_streaming(app, child, StopKind::Kill);
    Ok(pid)
}

fn connect_tun(app: &AppHandle, bin: &Path, cfg: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("TUN mode is currently only supported on macOS.".to_string());
    }

    // Passwordless path: the sudoers rule is installed.
    if privileges_installed(bin, cfg) {
        let child = Command::new("sudo")
            .arg("-n")
            .arg(bin)
            .arg("client")
            .arg("-c")
            .arg(cfg)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start hysteria via sudo: {e}"))?;
        spawn_streaming(app, child, StopKind::SudoPkill(cfg.to_path_buf()));
        return Ok(());
    }

    connect_tun_osascript(app, bin, cfg)
}

/// Fallback: launch as root via a one-off admin prompt, supervised by a logfile.
fn connect_tun_osascript(app: &AppHandle, bin: &Path, cfg: &Path) -> Result<(), String> {
    let dir = runtime_dir(app)?;
    let logfile = dir.join("hysteria-tun.log");
    let pidfile = dir.join("hysteria-tun.pid");
    let _ = fs::write(&logfile, "");
    let _ = fs::remove_file(&pidfile);

    let shell_cmd = format!(
        "'{bin}' client -c '{cfg}' > '{log}' 2>&1 & echo $! > '{pid}'",
        bin = bin.display(),
        cfg = cfg.display(),
        log = logfile.display(),
        pid = pidfile.display(),
    );
    run_osascript_admin(&shell_cmd)?;

    {
        let state = app.state::<RunnerState>();
        *state.active.lock().unwrap() = Active::TunOsascript { pidfile: pidfile.clone() };
    }
    spawn_tun_watcher(app, logfile, pidfile);
    Ok(())
}

/// Tail the osascript-tun logfile and watch for the root process exiting.
fn spawn_tun_watcher(app: &AppHandle, logfile: PathBuf, pidfile: PathBuf) {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<RunnerState>();
        *state.stop.lock().unwrap() = Some(stop.clone());
    }
    let app = app.clone();
    thread::spawn(move || {
        let mut pos: u64 = 0;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if let Ok(mut f) = File::open(&logfile) {
                let _ = f.seek(SeekFrom::Start(pos));
                let mut buf = String::new();
                if let Ok(n) = f.read_to_string(&mut buf) {
                    if n > 0 {
                        pos += n as u64;
                        for line in buf.lines() {
                            process_child_line(&app, line);
                        }
                    }
                }
            }
            if !pid_alive(&pidfile) {
                let state = app.state::<RunnerState>();
                let mut active = state.active.lock().unwrap();
                if matches!(&*active, Active::TunOsascript { .. }) {
                    *active = Active::None;
                    drop(active);
                    handle_unexpected_exit(&app);
                }
                break;
            }
            thread::sleep(Duration::from_millis(700));
        }
    });
}

fn pid_alive(pidfile: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(pidfile) else {
        return false;
    };
    let pid = contents.trim();
    if pid.is_empty() {
        return false;
    }
    // `ps -p` reports existence regardless of the process owner (unlike kill -0).
    Command::new("ps")
        .arg("-p")
        .arg(pid)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Stop the active connection.
pub fn disconnect(app: &AppHandle) -> Result<(), String> {
    stop_traffic_monitor(app);
    {
        let state = app.state::<RunnerState>();
        let stop = state.stop.lock().unwrap().take();
        if let Some(stop) = stop {
            stop.store(true, Ordering::Relaxed);
        }
        *state.last_error.lock().unwrap() = None;
    }

    let active = {
        let state = app.state::<RunnerState>();
        let mut guard = state.active.lock().unwrap();
        std::mem::replace(&mut *guard, Active::None)
    };

    match active {
        Active::None => {}
        Active::Child { mut child, stop } => {
            emit_log(app, "Disconnecting: stopping hysteria…");
            match stop {
                StopKind::Kill => {
                    let _ = child.kill();
                }
                StopKind::SudoPkill(cfg) => {
                    let _ = Command::new("sudo")
                        .arg("-n")
                        .arg("/usr/bin/pkill")
                        .arg("-f")
                        .arg(&cfg)
                        .status();
                }
            }
            let _ = child.wait();
            emit_log(app, "Disconnected.");
        }
        Active::TunOsascript { pidfile } => {
            emit_log(app, "Disconnecting: stopping hysteria…");
            kill_tun_osascript(&pidfile)?;
            emit_log(app, "Disconnected.");
        }
    }
    set_status(app, StatusPayload::disconnected());
    Ok(())
}

fn kill_tun_osascript(pidfile: &Path) -> Result<(), String> {
    let pid = fs::read_to_string(pidfile)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    if pid.is_empty() {
        return Ok(());
    }
    // Killing a root process needs root, so this re-prompts for authorization.
    run_osascript_admin(&format!("kill {pid}"))?;
    let _ = fs::remove_file(pidfile);
    Ok(())
}

pub fn current_status(app: &AppHandle) -> StatusPayload {
    let state = app.state::<RunnerState>();
    let status = state.status.lock().unwrap().clone();
    status
}

/// The last (profile, mode) passed to `connect`, for tray reconnects.
pub fn last_connection(app: &AppHandle) -> Option<(Profile, String)> {
    let state = app.state::<RunnerState>();
    let last = state.last_conn.lock().unwrap().clone();
    last
}

// --- External IP -----------------------------------------------------------

/// Externally visible IP via curl. In proxy mode the request must go through
/// the local listener (nothing routes app traffic automatically); in TUN mode
/// — or disconnected — a direct request already uses the system routes.
pub fn external_ip(app: &AppHandle) -> Result<String, String> {
    let state = app.state::<RunnerState>();
    let status = state.status.lock().unwrap().clone();
    let proxy = if status.state == "connected" && status.mode.as_deref() == Some("proxy") {
        let last = state.last_conn.lock().unwrap();
        last.as_ref().and_then(|(p, _)| proxy_url(&p.config))
    } else {
        None
    };

    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "--max-time", "5"]);
    if let Some(proxy) = proxy {
        cmd.arg("--proxy").arg(proxy);
    }
    cmd.arg("https://api.ipify.org");
    let out = cmd.output().map_err(|e| format!("Failed to run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "IP lookup failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if ip.is_empty() {
        return Err("IP lookup returned an empty response".to_string());
    }
    Ok(ip)
}

/// `curl --proxy` URL for whichever local listener the config enables.
/// `socks5h` (not `socks5`) so DNS resolves remotely too.
fn proxy_url(cfg: &HysteriaConfig) -> Option<String> {
    fn auth(user: &str, pass: &str) -> String {
        if user.is_empty() {
            String::new()
        } else {
            format!("{user}:{pass}@")
        }
    }
    if cfg.socks5.enabled && !cfg.socks5.listen.is_empty() {
        Some(format!(
            "socks5h://{}{}",
            auth(&cfg.socks5.username, &cfg.socks5.password),
            cfg.socks5.listen
        ))
    } else if cfg.http.enabled && !cfg.http.listen.is_empty() {
        Some(format!(
            "http://{}{}",
            auth(&cfg.http.username, &cfg.http.password),
            cfg.http.listen
        ))
    } else {
        None
    }
}

// --- Traffic monitor -----------------------------------------------------------

fn stop_traffic_monitor(app: &AppHandle) {
    let state = app.state::<RunnerState>();
    let stop = state.traffic_stop.lock().unwrap().take();
    if let Some(stop) = stop {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Poll OS byte counters once a second while the connection is up and push
/// session totals + rates to the UI. The hysteria *client* exposes no traffic
/// API (trafficStats is server-only), so the numbers come from the system:
/// interface counters for TUN, per-process counters for proxy mode.
#[cfg(target_os = "macos")]
fn spawn_traffic_monitor(app: &AppHandle, source: TrafficSource) {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<RunnerState>();
        *state.traffic_stop.lock().unwrap() = Some(stop.clone());
    }
    let app = app.clone();
    thread::spawn(move || {
        // First successful sample becomes the session baseline (interface
        // counters run since boot).
        let mut base: Option<(u64, u64)> = None;
        let mut prev: Option<(u64, u64, Instant)> = None;
        while !stop.load(Ordering::Relaxed) {
            if let Some((rx, tx)) = sample_traffic(&source) {
                let (brx, btx) = *base.get_or_insert((rx, tx));
                let now = Instant::now();
                let (rx_rate, tx_rate) = match prev {
                    Some((prx, ptx, pt)) => {
                        let dt = now.duration_since(pt).as_secs_f64().max(0.001);
                        (
                            rx.saturating_sub(prx) as f64 / dt,
                            tx.saturating_sub(ptx) as f64 / dt,
                        )
                    }
                    None => (0.0, 0.0),
                };
                prev = Some((rx, tx, now));
                let _ = app.emit(
                    TRAFFIC_EVENT,
                    TrafficPayload {
                        rx: rx.saturating_sub(brx),
                        tx: tx.saturating_sub(btx),
                        rx_rate,
                        tx_rate,
                    },
                );
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn spawn_traffic_monitor(_app: &AppHandle, _source: TrafficSource) {}

/// Cumulative (rx, tx) bytes for the source, or None if unavailable (e.g. the
/// tun interface hasn't been created yet).
#[cfg(target_os = "macos")]
fn sample_traffic(source: &TrafficSource) -> Option<(u64, u64)> {
    match source {
        TrafficSource::Interface(name) => {
            let out = Command::new("netstat").arg("-ibn").output().ok()?;
            parse_netstat_interface(&String::from_utf8_lossy(&out.stdout), name)
        }
        TrafficSource::Process(pid) => {
            let out = Command::new("nettop")
                .args(["-x", "-l", "1", "-J", "bytes_in,bytes_out", "-p"])
                .arg(pid.to_string())
                .output()
                .ok()?;
            parse_nettop_process(&String::from_utf8_lossy(&out.stdout), *pid)
        }
    }
}

/// Extract (Ibytes, Obytes) for an interface from `netstat -ibn` output. The
/// Address column can be blank, so columns are indexed from the line's end:
/// ... Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll.
fn parse_netstat_interface(text: &str, ifname: &str) -> Option<(u64, u64)> {
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.first() != Some(&ifname) || fields.len() < 8 {
            continue;
        }
        let n = fields.len();
        if let (Ok(ibytes), Ok(obytes)) =
            (fields[n - 5].parse::<u64>(), fields[n - 2].parse::<u64>())
        {
            return Some((ibytes, obytes));
        }
    }
    None
}

/// Extract (bytes_in, bytes_out) for a process from
/// `nettop -x -l 1 -J bytes_in,bytes_out -p <pid>` output. The process row
/// starts at column 0 as `name.<pid>`; indented rows are its connections.
fn parse_nettop_process(text: &str, pid: u32) -> Option<(u64, u64)> {
    let suffix = format!(".{pid}");
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 || !fields[0].ends_with(&suffix) {
            continue;
        }
        if let (Ok(rx), Ok(tx)) = (fields[1].parse::<u64>(), fields[2].parse::<u64>()) {
            return Some((rx, tx));
        }
    }
    None
}

// --- TUN config preparation ---------------------------------------------------

/// Fix up the tun config just before launch: assign a valid interface name
/// (macOS requires `utun<number>`) and exclude the server's own IP(s) from the
/// tunnel so the proxy connection itself isn't routed back into it.
fn prepare_tun(cfg: &mut HysteriaConfig) {
    #[cfg(target_os = "macos")]
    {
        cfg.tun.name = normalize_utun_name(&cfg.tun.name);
    }
    if cfg.tun.exclude_routes.is_empty() {
        cfg.tun.exclude_routes = resolve_server_excludes(&cfg.server);
    }
}

/// Resolve `host:port` to the set of server IP CIDRs to exclude from the tunnel.
fn resolve_server_excludes(server: &str) -> Vec<String> {
    use std::net::ToSocketAddrs;
    let mut out: Vec<String> = Vec::new();
    if let Ok(addrs) = server.to_socket_addrs() {
        for a in addrs {
            let ip = a.ip();
            let cidr = if ip.is_ipv4() {
                format!("{ip}/32")
            } else {
                format!("{ip}/128")
            };
            if !out.contains(&cidr) {
                out.push(cidr);
            }
        }
    }
    out
}

/// Pick a valid macOS tun name. Keeps a user-supplied `utun<number>`, otherwise
/// chooses the lowest `utunN` index not already present on the system.
#[cfg(target_os = "macos")]
fn normalize_utun_name(requested: &str) -> String {
    if let Some(rest) = requested.strip_prefix("utun") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return requested.to_string();
        }
    }
    let existing = existing_utun_indices();
    let mut i = 0u32;
    while existing.contains(&i) {
        i += 1;
    }
    format!("utun{i}")
}

#[cfg(target_os = "macos")]
fn existing_utun_indices() -> std::collections::HashSet<u32> {
    Command::new("ifconfig")
        .arg("-l")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.split_whitespace()
                .filter_map(|n| n.strip_prefix("utun").and_then(|d| d.parse::<u32>().ok()))
                .collect()
        })
        .unwrap_or_default()
}

// --- Privileged helper (one-time sudoers rule) --------------------------------

/// Check whether the exact `sudo hysteria client -c <cfg>` command is permitted
/// without a password. Uses `sudo -n -l`, which never prompts.
fn privileges_installed(bin: &Path, cfg: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    Command::new("sudo")
        .arg("-n")
        .arg("-l")
        .arg(bin)
        .arg("client")
        .arg("-c")
        .arg(cfg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// UI query: is the passwordless rule currently active for our paths?
pub fn privilege_status(app: &AppHandle) -> bool {
    let (Ok(bin), Ok(cfg)) = (hysteria_path(app), active_config_path(app)) else {
        return false;
    };
    privileges_installed(&bin, &cfg)
}

fn current_user() -> Result<String, String> {
    if let Ok(u) = std::env::var("USER") {
        if !u.is_empty() {
            return Ok(u);
        }
    }
    let out = Command::new("id")
        .arg("-un")
        .output()
        .map_err(|e| e.to_string())?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        Err("Could not determine current user".to_string())
    } else {
        Ok(name)
    }
}

/// Install the one-time sudoers rule (single admin prompt). Pins NOPASSWD to the
/// exact hysteria binary + active-config path, plus the matching pkill used to
/// stop it.
pub fn install_privileges(app: &AppHandle) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Passwordless setup is only supported on macOS.".to_string());
    }
    let bin = hysteria_path(app)?;
    let cfg = active_config_path(app)?;
    let user = current_user()?;

    let bin_s = bin.to_string_lossy().to_string();
    let cfg_s = cfg.to_string_lossy().to_string();
    // sudoers cannot match command paths that contain whitespace.
    if bin_s.contains(char::is_whitespace) || cfg_s.contains(char::is_whitespace) {
        return Err(format!(
            "A required path contains a space, which sudoers can't match:\n{bin_s}\n{cfg_s}"
        ));
    }

    let content = format!(
        "# Managed by Hysteria Client UI — allows running the bundled hysteria\n\
         # binary as root for TUN mode without a password. Remove to revoke.\n\
         {user} ALL=(root) NOPASSWD: {bin_s} client -c {cfg_s}\n\
         {user} ALL=(root) NOPASSWD: /usr/bin/pkill -f {cfg_s}\n"
    );

    let tmp = runtime_dir(app)?.join("hysteria-ui.sudoers");
    fs::write(&tmp, &content).map_err(|e| e.to_string())?;

    // Validate syntax before touching the real file.
    let check = Command::new("visudo")
        .arg("-cf")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("visudo not available: {e}"))?;
    if !check.status.success() {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "Generated sudoers rule failed validation: {}",
            String::from_utf8_lossy(&check.stderr)
        ));
    }

    // Install as root:wheel 0440 and re-validate, in one privileged step.
    let install_cmd = format!(
        "/usr/bin/install -m 0440 -o root -g wheel '{tmp}' '{dest}' && /usr/sbin/visudo -cf '{dest}'",
        tmp = tmp.display(),
        dest = SUDOERS_FILE,
    );
    let result = run_osascript_admin(&install_cmd);
    let _ = fs::remove_file(&tmp);
    result
}

/// Remove the sudoers rule (single admin prompt).
pub fn uninstall_privileges() -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    run_osascript_admin(&format!("/bin/rm -f '{SUDOERS_FILE}'"))
}

// --- osascript helpers --------------------------------------------------------

/// Run a shell command as root via a single macOS authorization prompt.
fn run_osascript_admin(shell_cmd: &str) -> Result<(), String> {
    let apple_script = format!(
        "do shell script \"{}\" with administrator privileges",
        escape_applescript(shell_cmd)
    );
    let status = Command::new("osascript")
        .arg("-e")
        .arg(&apple_script)
        .status()
        .map_err(|e| format!("Failed to invoke osascript: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Authorization was cancelled or failed.".to_string())
    }
}

/// Escape a string for embedding inside an AppleScript double-quoted literal.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{
        apply_mode, classify_line, error_message, parse_connect_count, parse_netstat_interface,
        parse_nettop_process, parse_version, proxy_url, strip_ansi, LineEvent,
    };
    use crate::config::generate_yaml;
    use crate::model::HysteriaConfig;

    fn cfg() -> HysteriaConfig {
        HysteriaConfig {
            server: "1.2.3.4:443".to_string(),
            ..Default::default()
        }
    }

    /// The tun block is derived from the mode, not stored in the profile, so a
    /// config that skips `apply_mode` renders YAML with no tun section at all —
    /// which is what made the "View YAML" preview ignore the Client settings.
    #[test]
    fn tun_mode_settings_reach_the_yaml() {
        let mut c = cfg();
        c.tun.mtu = 9000;
        c.tun.dns = vec!["1.1.1.1".to_string()];
        assert!(!generate_yaml(&c).unwrap().contains("tun:"));

        apply_mode(&mut c, "tun").unwrap();
        let yaml = generate_yaml(&c).unwrap();
        assert!(yaml.contains("tun:"));
        assert!(yaml.contains("9000"));
        assert!(yaml.contains("1.1.1.1"));
        // prepare_tun keeps the server's own IP out of the tunnel.
        assert!(yaml.contains("1.2.3.4/32"));
    }

    #[test]
    fn proxy_mode_falls_back_to_a_socks5_listener() {
        let mut c = cfg();
        apply_mode(&mut c, "proxy").unwrap();
        assert_eq!(c.socks5.listen, "127.0.0.1:1080");
        let yaml = generate_yaml(&c).unwrap();
        assert!(yaml.contains("socks5:"));
        assert!(!yaml.contains("tun:"));
    }

    /// Mode is the single switch: a profile saved while in tun mode must not
    /// drag a tun block into a proxy-mode config.
    #[test]
    fn mode_overrides_a_stale_tun_flag() {
        let mut c = cfg();
        c.tun.enabled = true;
        apply_mode(&mut c, "proxy").unwrap();
        assert!(!generate_yaml(&c).unwrap().contains("tun:"));
    }

    #[test]
    fn reads_the_version_past_the_banner() {
        let out = concat!(
            "\u{2591}\u{2588} banner \u{2588}\u{2591}\n\n",
            "a powerful, lightning fast and censorship resistant proxy\n\n",
            "Version:\tv2.12.2\n",
            "BuildDate:\t2026-08-23T00:39:00Z\n",
            "Platform:\tdarwin\n",
        );
        assert_eq!(parse_version(out).as_deref(), Some("v2.12.2"));
        assert_eq!(parse_version("no version here\n"), None);
        assert_eq!(parse_version("Version:\t\n"), None);
    }

    #[test]
    fn rejects_unknown_mode() {
        assert!(apply_mode(&mut cfg(), "banana").is_err());
    }

    #[test]
    fn classifies_handshake_and_fatal_lines() {
        assert_eq!(
            classify_line(r#"2025-07-14T10:00:01+03:00 INFO connected to server {"count": 1}"#),
            LineEvent::Connected
        );
        assert_eq!(
            classify_line(
                r#"2025-07-14T10:00:05+03:00 FATAL failed to initialize client {"error": "timeout"}"#
            ),
            LineEvent::Fatal
        );
        assert_eq!(classify_line("FATAL failed to load client config"), LineEvent::Fatal);
        assert_eq!(
            classify_line(r#"INFO SOCKS5 server listening {"addr": "127.0.0.1:1080"}"#),
            LineEvent::None
        );
        // Per-request runtime errors must not flip the connection state.
        assert_eq!(
            classify_line(r#"ERROR TCP error {"addr": "x.com:443", "error": "timeout"}"#),
            LineEvent::None
        );
    }

    #[test]
    fn parses_connect_count() {
        assert_eq!(
            parse_connect_count(
                r#"2025-07-14T10:00:01+03:00 INFO connected to server {"addr": "1.2.3.4:443", "udpEnabled": true, "count": 1}"#
            ),
            Some(1)
        );
        assert_eq!(parse_connect_count(r#"connected to server {"count": 12}"#), Some(12));
        // zap may put a space after the colon depending on encoder config.
        assert_eq!(parse_connect_count(r#"connected to server {"count": 3, "tx": 0}"#), Some(3));
        assert_eq!(parse_connect_count("connected to server"), None);
    }

    #[test]
    fn error_message_strips_timestamp() {
        assert_eq!(
            error_message("2025-07-14T10:00:05+03:00 FATAL failed to initialize client"),
            "FATAL failed to initialize client"
        );
        assert_eq!(error_message("  plain error  "), "plain error");
    }

    #[test]
    fn parses_netstat_interface_counters() {
        // Real `netstat -ibn` shape: the Link row has a blank Address column.
        let text = "\
Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
lo0        16384 <Link#1>                      78890543     0 307414530332 78890543     0 307414530332     0
utun4      1400  <Link#20>                          100     0       12345      200     0       67890     0
utun4      1400  198.18.0/15   198.18.0.1           100     -       12345      200     -       67890     -
";
        assert_eq!(parse_netstat_interface(text, "utun4"), Some((12345, 67890)));
        assert_eq!(parse_netstat_interface(text, "utun9"), None);
    }

    #[test]
    fn parses_nettop_process_counters() {
        let text = "\
                                                                     bytes_in       bytes_out
hysteria.4242                                                         1043938            5150
   udp4 *:55555<->1.2.3.4:443                                         1043938            5150
";
        assert_eq!(parse_nettop_process(text, 4242), Some((1043938, 5150)));
        assert_eq!(parse_nettop_process(text, 9999), None);
    }

    #[test]
    fn proxy_url_prefers_socks5_and_carries_auth() {
        let mut c = cfg();
        apply_mode(&mut c, "proxy").unwrap(); // enables the default SOCKS5 listener
        assert_eq!(proxy_url(&c), Some("socks5h://127.0.0.1:1080".to_string()));

        c.socks5.username = "u".into();
        c.socks5.password = "p".into();
        assert_eq!(proxy_url(&c), Some("socks5h://u:p@127.0.0.1:1080".to_string()));

        c.socks5.enabled = false;
        c.http.enabled = true;
        c.http.listen = "127.0.0.1:8080".into();
        assert_eq!(proxy_url(&c), Some("http://127.0.0.1:8080".to_string()));

        c.http.enabled = false;
        assert_eq!(proxy_url(&c), None);
    }

    #[test]
    fn strips_color_codes() {
        assert_eq!(
            strip_ansi("\x1b[36mINFO\x1b[0m client mode: \x1b[1;32msocks5\x1b[0m"),
            "INFO client mode: socks5"
        );
    }

    #[test]
    fn strips_osc_and_bare_escapes() {
        assert_eq!(strip_ansi("\x1b]0;title\x07line \x1b7here"), "line here");
        assert_eq!(strip_ansi("\x1b]0;title\x1b\\line"), "line");
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(strip_ansi("connected to server ok"), "connected to server ok");
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn truncated_escape_at_end_does_not_panic() {
        assert_eq!(strip_ansi("abc\x1b"), "abc");
        assert_eq!(strip_ansi("abc\x1b["), "abc");
    }
}
