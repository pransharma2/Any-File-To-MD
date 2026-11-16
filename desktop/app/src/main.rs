// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shell;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anytomd_engine as engine;
use serde::Serialize;
use tauri::{LogicalPosition, LogicalSize, Manager, State, WebviewWindow};

struct AppState {
    job: Mutex<Arc<engine::Job>>,
    launch: Option<String>,
}

impl AppState {
    fn job(&self) -> Arc<engine::Job> {
        self.job.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

fn clean(p: &str) -> PathBuf {
    PathBuf::from(p.trim().trim_matches('"'))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchInfo {
    source: Option<String>,
    version: &'static str,
    shell_registered: bool,
    shell_supported: bool,
}

#[tauri::command]
fn launch_info(state: State<'_, AppState>) -> LaunchInfo {
    LaunchInfo {
        source: state.launch.clone(),
        version: engine::VERSION,
        shell_registered: shell::is_registered(),
        shell_supported: shell::SUPPORTED,
    }
}

#[tauri::command]
fn suggest_output(source: String) -> String {
    if source.trim().is_empty() {
        return String::new();
    }
    engine::default_output(&clean(&source)).display().to_string()
}

#[tauri::command]
fn path_kind(path: String) -> &'static str {
    let p = clean(&path);
    if p.is_dir() {
        "dir"
    } else if p.is_file() {
        "file"
    } else {
        "missing"
    }
}

#[derive(Serialize)]
struct Listing {
    files: Vec<engine::Listed>,
    truncated: bool,
}

/// Every file the options would convert, for the picker in the mirror preview.
#[tauri::command]
async fn list_source(source: String, output: String, options: engine::Options) -> Result<Listing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = clean(&source);
        let out = if output.trim().is_empty() { engine::default_output(&src) } else { clean(&output) };
        engine::list_files(&src, &out, &options, 20_000).map(|(files, truncated)| Listing { files, truncated })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_folder(window: WebviewWindow, title: String, start: Option<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = rfd::FileDialog::new().set_title(&title).set_parent(&window);
        if let Some(s) = start.filter(|s| Path::new(s.trim()).is_dir()) {
            d = d.set_directory(s.trim());
        }
        d.pick_folder().map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
fn start_conversion(
    state: State<'_, AppState>,
    source: String,
    output: String,
    options: engine::Options,
) -> Result<String, String> {
    let src = clean(&source);
    let out = if output.trim().is_empty() { engine::default_output(&src) } else { clean(&output) };
    if let Some(e) = engine::validate(&src, &out) {
        return Err(e);
    }
    let mut guard = state.job.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_running() {
        return Err("A conversion is already running.".into());
    }
    let job = engine::Job::new();
    *guard = job.clone();
    let shown = out.display().to_string();
    std::thread::spawn(move || engine::convert_folder(&src, &out, &options, &job));
    Ok(shown)
}

#[derive(Serialize)]
struct StatusReply {
    status: engine::Status,
    items: Vec<engine::Item>,
    next: usize,
}

#[tauri::command]
fn get_status(state: State<'_, AppState>, since: usize) -> StatusReply {
    let (status, items, next) = state.job().snapshot(since);
    StatusReply { status, items, next }
}

#[tauri::command]
fn cancel(state: State<'_, AppState>) {
    state.job().cancel();
}

/// Opens the output folder (or the combined file) of the current job only.
#[tauri::command]
fn open_output(state: State<'_, AppState>, combined: bool) -> Result<(), String> {
    let (s, _, _) = state.job().snapshot(usize::MAX);
    let target = if combined { s.combined.unwrap_or_default() } else { s.output };
    if target.is_empty() || !Path::new(&target).exists() {
        return Err("Nothing to open yet.".into());
    }
    open_in_shell(&target)
}

fn open_in_shell(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn window_control(window: WebviewWindow, action: String) -> Result<(), String> {
    let r = match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "close" => window.close(),
        _ => Ok(()),
    };
    r.map_err(|e| e.to_string())
}

#[tauri::command]
fn set_shell_integration(enabled: bool) -> Result<bool, String> {
    if enabled {
        shell::register()?;
    } else {
        shell::unregister()?;
    }
    Ok(shell::is_registered())
}

/// The design is 1024×720, but a 1080p screen at 150% scaling only has ~680px above the
/// taskbar. Shrink to the usable area (keeping a margin) and re-center.
fn fit_to_screen(w: &WebviewWindow) {
    let Ok(Some(mon)) = w.current_monitor() else { return };
    let scale = mon.scale_factor();
    let area = mon.work_area();
    let (avail_w, avail_h) = (area.size.width as f64 / scale, area.size.height as f64 / scale);
    let width = 1024f64.min(avail_w - 24.0);
    let height = 720f64.min(avail_h - 24.0);
    let _ = w.set_min_size(Some(LogicalSize::new(width.min(960.0), height.min(600.0))));
    let _ = w.set_size(LogicalSize::new(width, height));
    let x = area.position.x as f64 / scale + (avail_w - width) / 2.0;
    let y = area.position.y as f64 / scale + (avail_h - height) / 2.0;
    let _ = w.set_position(LogicalPosition::new(x, y));
}

/// Right-click on a single file: convert silently, only speak up on failure.
fn convert_file_quietly(path: &Path) {
    if let Err(e) = engine::convert_single_file(path, None) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Any File To MD")
            .set_description(format!("Couldn't convert {name}.\n\n{e}"))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut launch: Option<String> = None;
    match args.first().map(String::as_str) {
        Some("--register-shell") => {
            let _ = shell::register();
            return;
        }
        Some("--unregister-shell") => {
            let _ = shell::unregister();
            return;
        }
        Some("--convert") | Some(_) => {
            // "--convert <path>" comes from the Explorer menu; a bare path from dropping onto the .exe.
            let raw = if args[0] == "--convert" { args.get(1).cloned() } else { Some(args[0].clone()) };
            if let Some(p) = raw {
                let path = clean(&p);
                if path.is_file() {
                    std::panic::set_hook(Box::new(|_| {}));
                    convert_file_quietly(&path);
                    return;
                }
                if path.is_dir() {
                    launch = Some(path.display().to_string());
                }
            }
        }
        None => {}
    }
    // Parser panics are caught per file; keep them from printing.
    std::panic::set_hook(Box::new(|_| {}));

    tauri::Builder::default()
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                fit_to_screen(&w);
            }
            Ok(())
        })
        .manage(AppState { job: Mutex::new(engine::Job::new()), launch })
        .invoke_handler(tauri::generate_handler![
            launch_info,
            suggest_output,
            path_kind,
            list_source,
            pick_folder,
            start_conversion,
            get_status,
            cancel,
            open_output,
            window_control,
            set_shell_integration
        ])
        .run(tauri::generate_context!())
        .expect("error while running Any File To MD");
}
