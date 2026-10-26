//! End-to-end check of a packaged layout, run by CI on Windows before the installer is built:
//! starts PostgreSQL, the model server and Yusr from the bundled files, signs in with the
//! first-run password, asks the advisor one question, then shuts everything down.
//!
//! `cargo run -p yusr-launcher --example smoke -- <resources dir> [--no-model]`

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let res = PathBuf::from(args.next().ok_or("usage: smoke <resources dir> [--no-model]")?);
    let with_model = args.next().as_deref() != Some("--no-model");
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let data = std::env::temp_dir().join(format!("yusr-smoke-{}", yusr_launcher::random_secret(6)));
    let layout = yusr_launcher::Layout {
        data: data.clone(),
        pg_bin: res.join("postgresql").join("bin"),
        ui_dir: res.join("web"),
        llama_server: with_model.then(|| res.join("llama").join(format!("llama-server{exe}"))),
        model: with_model.then(|| res.join("models").join("advisor.gguf")),
        model_download: None,
    };
    let running = yusr_launcher::start(layout).await?;
    let result = check(&running.url, running.first_admin_password.as_deref().ok_or("no first-run password")?, with_model).await;
    running.shutdown();
    let _ = std::fs::remove_dir_all(&data);
    result?;
    println!("smoke test passed");
    Ok(())
}

async fn check(url: &str, password: &str, with_model: bool) -> Result<(), String> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(300)).build().map_err(|e| e.to_string())?;
    let get = |path: &str| http.get(format!("{url}{path}"));
    let page = get("/").send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    if !page.contains("<div id=\"root\">") {
        return Err("the interface is not served".into());
    }
    let login: Value = http
        .post(format!("{url}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": password }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let token = login["token"].as_str().ok_or(format!("login failed: {login}"))?.to_owned();
    let auth = |r: reqwest::RequestBuilder| r.bearer_auth(&token);
    let verify: Value = auth(get("/api/audit/verify")).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    if verify["ok"] != json!(true) {
        return Err(format!("audit chain: {verify}"));
    }
    let backup = auth(get("/api/backup")).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    if !backup.starts_with(b"YUSRBAK1") {
        return Err("backup download failed".into());
    }
    if with_model {
        // The model server may still be loading the weights.
        let mut ready = false;
        for _ in 0..90 {
            let s: Value = auth(get("/api/advisor/status")).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
            if s["reachable"] == json!(true) {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        if !ready {
            return Err("the advisor model never became reachable".into());
        }
        let answer = auth(http.post(format!("{url}/api/advisor/chat")))
            .json(&json!({ "messages": [{ "role": "user", "content": "ما الصفحة التي أسجّل منها تبرعاً جديداً؟ أجب في سطر." }] }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = answer.status();
        let text = answer.text().await.map_err(|e| e.to_string())?;
        println!("advisor ({status}): {text}");
        if !status.is_success() || text.trim().is_empty() {
            return Err("the advisor gave no answer".into());
        }
    }
    Ok(())
}
