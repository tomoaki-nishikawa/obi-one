use crate::core::clamp_cut_ratio;
use crate::processor::{process_pdf, ProcessRequest, ProcessResult};
use crate::settings::{self, AppSettings, TemplateMode};
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<AppSettings, String> {
    let config_dir = app.path().app_config_dir().map_err(to_string)?;
    let mut settings = settings::load(&config_dir).map_err(to_string)?;
    settings.strip_obi = true;
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.obi_cut_ratio = clamp_cut_ratio(settings.obi_cut_ratio);
    settings.strip_obi = true;
    let config_dir = app.path().app_config_dir().map_err(to_string)?;
    settings::save(&config_dir, &settings).map_err(to_string)?;
    Ok(settings)
}

#[tauri::command]
pub async fn choose_input_pdfs(app: AppHandle) -> Result<Vec<String>, String> {
    let (tx, rx) = mpsc::channel();
    app
        .dialog()
        .file()
        .set_title("物件PDFを選択")
        .add_filter("PDF", &["pdf"])
        .pick_files(move |paths| {
            let _ = tx.send(paths);
        });

    let paths = wait_dialog(rx).await?;
    Ok(paths
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| path.into_path().ok())
        .map(path_to_string)
        .collect())
}

#[tauri::command]
pub async fn choose_output_dir(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = mpsc::channel();
    app
        .dialog()
        .file()
        .set_title("出力先フォルダを選択")
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });

    Ok(wait_dialog(rx)
        .await?
        .and_then(|path| path.into_path().ok())
        .map(path_to_string))
}

#[tauri::command]
pub async fn choose_obi_band_image(app: AppHandle, mut settings: AppSettings) -> Result<AppSettings, String> {
    let (tx, rx) = mpsc::channel();
    app
        .dialog()
        .file()
        .set_title("帯画像を選択")
        .add_filter("画像", &["jpg", "jpeg", "png"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });

    let selected = wait_dialog(rx)
        .await?
        .and_then(|path| path.into_path().ok());

    if let Some(path) = selected {
        settings.template_mode = TemplateMode::Custom;
        settings.custom_template_path = Some(path_to_string(path));
        save_settings(app, settings.clone())?;
    }

    Ok(settings)
}

#[tauri::command]
pub fn reset_obi_band(app: AppHandle, mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.template_mode = TemplateMode::Bundled;
    settings.custom_template_path = None;
    save_settings(app, settings.clone())?;
    Ok(settings)
}

#[tauri::command]
pub async fn process_one(app: AppHandle, request: ProcessRequest) -> Result<ProcessResult, String> {
    tauri::async_runtime::spawn_blocking(move || process_pdf(&app, request))
        .await
        .map_err(to_string)?
        .map_err(to_string)
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    open_with_os(&PathBuf::from(path)).map_err(to_string)
}

fn open_with_os(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("フォルダが見つかりません: {}", path.display()));
    }

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .with_context(|| format!("フォルダを開けませんでした: {}", path.display()))?;
    Ok(())
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

async fn wait_dialog<T: Send + 'static>(rx: mpsc::Receiver<T>) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv()
            .map_err(|_| "選択ダイアログの結果を受け取れませんでした。".to_string())
    })
    .await
    .map_err(to_string)?
}

fn to_string<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}
