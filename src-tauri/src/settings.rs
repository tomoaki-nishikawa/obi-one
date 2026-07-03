use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("設定フォルダを作成できませんでした: {0}")]
    CreateDir(String),
    #[error("設定ファイルを読み込めませんでした: {0}")]
    Read(String),
    #[error("設定ファイルを書き込めませんでした: {0}")]
    Write(String),
    #[error("設定ファイルの形式が不正です: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateMode {
    Bundled,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub output_dir: Option<String>,
    pub template_mode: TemplateMode,
    pub custom_template_path: Option<String>,
    pub emit_pdf: bool,
    pub emit_jpeg: bool,
    pub strip_obi: bool,
    pub obi_cut_ratio: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            output_dir: None,
            template_mode: TemplateMode::Bundled,
            custom_template_path: None,
            emit_pdf: true,
            emit_jpeg: false,
            strip_obi: true,
            obi_cut_ratio: 0.20,
        }
    }
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(config_dir: &Path) -> Result<AppSettings, SettingsError> {
    let path = settings_path(config_dir);
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = fs::read_to_string(&path).map_err(|e| SettingsError::Read(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| SettingsError::Parse(e.to_string()))
}

pub fn save(config_dir: &Path, settings: &AppSettings) -> Result<(), SettingsError> {
    fs::create_dir_all(config_dir).map_err(|e| SettingsError::CreateDir(e.to_string()))?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| SettingsError::Write(e.to_string()))?;
    fs::write(settings_path(config_dir), raw).map_err(|e| SettingsError::Write(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_match_initial_app_behavior() {
        let settings = AppSettings::default();
        assert!(settings.emit_pdf);
        assert!(!settings.emit_jpeg);
        assert!(settings.strip_obi);
        assert_eq!(settings.template_mode, TemplateMode::Bundled);
    }

    #[test]
    fn saves_and_loads_settings() {
        let dir = tempfile::tempdir().unwrap();
        let settings = AppSettings {
            output_dir: Some("/tmp/out".into()),
            emit_jpeg: true,
            ..AppSettings::default()
        };

        save(dir.path(), &settings).unwrap();
        assert_eq!(load(dir.path()).unwrap(), settings);
    }
}
