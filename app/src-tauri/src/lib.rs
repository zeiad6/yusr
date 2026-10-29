//! The Yusr desktop shell: one window showing the splash page while yusr-launcher starts the
//! bundled PostgreSQL, model server and Yusr server, then the app itself.

use std::sync::Mutex;

use tauri::webview::NewWindowResponse;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use yusr_launcher::{Layout, Running};

const MAIN: &str = "main";

/// Held so the exit handler can stop PostgreSQL and the model server cleanly.
struct Services(Mutex<Option<Running>>);

fn layout(app: &tauri::AppHandle) -> Result<Layout, String> {
    let raw_res = app.path().resource_dir().map_err(|e| e.to_string())?;
    let res = yusr_launcher::paths::clean_path(raw_res);
    #[cfg(feature = "lite")]
    let (model, model_download, llama_server) = (None, None, None);

    #[cfg(not(feature = "lite"))]
    let (model, model_download, llama_server) = {
        let exe = if cfg!(windows) { ".exe" } else { "" };
        let bundled_model = res.join("models").join("advisor.gguf");
        let (m, md) = if bundled_model.exists() {
            (Some(bundled_model), None)
        } else {
            (
                Some(yusr_launcher::paths::data_dir().join("models").join("advisor.gguf")),
                Some(yusr_launcher::model::ModelSource::advisor()),
            )
        };
        (m, md, Some(res.join("llama").join(format!("llama-server{exe}"))))
    };

    let pg_bin = yusr_launcher::paths::clean_path(res.join("postgresql").join("bin"));
    Ok(Layout {
        data: yusr_launcher::paths::data_dir(),
        pg_bin,
        ui_dir: res.join("web"),
        llama_server,
        model,
        model_download,
    })
}

/// Text placed into the splash page's script: JSON escaping makes any message safe there.
fn show_error(window: &tauri::WebviewWindow, message: &str) {
    let _ = window.eval(format!("window.showError({})", serde_json::Value::String(message.to_owned())));
}

/// Ensures the app and its database are registered to start automatically on system boot.
fn ensure_autostart_on_boot() {
    #[cfg(windows)]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.display().to_string();
            let _ = std::process::Command::new("reg")
                .args(&[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "Yusr",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{exe_str}\""),
                    "/f",
                ])
                .output();
        }
    }
    #[cfg(unix)]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Ok(home) = std::env::var("HOME") {
                let autostart_dir = std::path::PathBuf::from(home).join(".config").join("autostart");
                let _ = std::fs::create_dir_all(&autostart_dir);
                let desktop_file = autostart_dir.join("yusr.desktop");
                let content = format!(
                    "[Desktop Entry]\nType=Application\nName=Yusr\nExec=\"{}\"\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
                    exe_path.display()
                );
                let _ = std::fs::write(desktop_file, content);
            }
        }
    }
}

pub fn run() {
    ensure_autostart_on_boot();
    let data = yusr_launcher::paths::data_dir();
    let _ = std::fs::create_dir_all(data.join("logs"));
    let log = std::fs::File::create(data.join("logs").join("yusr.log")).ok();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into());
    match log {
        Some(file) => tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(Mutex::new(file)).init(),
        None => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }

    let app = tauri::Builder::default()
        // A second launch focuses the running window: two instances would fight over the
        // same database folder.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window(MAIN) {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .manage(Services(Mutex::new(None)))
        .setup(|app| {
            let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
                .title("يُسر — نظام إدارة الجمعيات الخيرية")
                .inner_size(1080.0, 680.0)
                .min_inner_size(880.0, 580.0)
                .center()
                // Receipts open in a new window from a blob: URL; let WebView2 create it.
                .on_new_window(|_url, _features| NewWindowResponse::Allow)
                .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Check if configured to run in Client Mode connecting to a remote LAN host
                let client_host = std::env::var("YUSR_HOST_URL").ok()
                    .or_else(|| std::fs::read_to_string(yusr_launcher::paths::data_dir().join("client_host.txt")).ok().map(|s| s.trim().to_owned()))
                    .filter(|s| !s.is_empty());

                if let Some(host_url) = client_host {
                    tracing::info!(host = %host_url, "launching in LAN Client Mode, connecting to remote host");
                    match host_url.parse() {
                        Ok(u) => {
                            if let Err(e) = window.navigate(u) {
                                show_error(&window, &e.to_string());
                            }
                            return;
                        }
                        Err(e) => show_error(&window, &format!("Invalid host URL: {e}")),
                    }
                }

                let started = match layout(&handle) {
                    Ok(l) => yusr_launcher::start(l).await,
                    Err(e) => Err(e),
                };
                match started {
                    Ok(running) => {
                        let mut url = format!("{}/login", running.url);
                        if let Some(pw) = &running.first_admin_password {
                            // A fragment never leaves the window, so the password is not sent anywhere.
                            url.push_str(&format!("#first-run={pw}"));
                        }
                        *handle.state::<Services>().0.lock().expect("services lock") = Some(running);
                        match url.parse() {
                            Ok(u) => {
                                if let Err(e) = window.navigate(u) {
                                    show_error(&window, &e.to_string());
                                }
                            }
                            Err(e) => show_error(&window, &format!("{e}")),
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "start failed");
                        show_error(&window, &e);
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the Yusr window could not be created");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(running) = handle.state::<Services>().0.lock().expect("services lock").take() {
                running.shutdown();
            }
        }
    });
}
