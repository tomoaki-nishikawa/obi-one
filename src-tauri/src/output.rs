use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("入力ファイル名を取得できませんでした。")]
    MissingFileName,
    #[error("出力ファイル名を作成できませんでした。")]
    MissingStem,
    #[error("出力先フォルダを作成できませんでした: {0}")]
    CreateDir(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputPlan {
    pub date_dir: PathBuf,
    pub pdf_path: Option<PathBuf>,
    pub jpeg_path: Option<PathBuf>,
}

pub fn date_folder_name(date: NaiveDate) -> String {
    format!("{:04}{:02}{:02}", date.year(), date.month(), date.day())
}

pub fn prefixed_stem(input_path: &Path) -> Result<String, OutputError> {
    let file_name = input_path.file_name().ok_or(OutputError::MissingFileName)?;
    let stem = Path::new(file_name)
        .file_stem()
        .ok_or(OutputError::MissingStem)?
        .to_string_lossy();
    Ok(format!("い）{}", sanitize_file_stem(&stem)))
}

pub fn sanitize_file_stem(stem: &str) -> String {
    let sanitized: String = stem
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();

    let trimmed = sanitized.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

pub fn plan_output_paths(
    output_root: &Path,
    input_path: &Path,
    emit_pdf: bool,
    emit_jpeg: bool,
    date: NaiveDate,
) -> Result<OutputPlan, OutputError> {
    let date_dir = output_root.join(date_folder_name(date));
    let base = prefixed_stem(input_path)?;
    let existing = list_existing_names(&date_dir);
    let suffix = choose_suffix(&existing, &base, emit_pdf, emit_jpeg);
    let stem = match suffix {
        1 => base,
        n => format!("{}_{}", base, n),
    };

    Ok(OutputPlan {
        date_dir: date_dir.clone(),
        pdf_path: emit_pdf.then(|| date_dir.join(format!("{stem}.pdf"))),
        jpeg_path: emit_jpeg.then(|| date_dir.join(format!("{stem}.jpg"))),
    })
}

pub fn ensure_output_dir(path: &Path) -> Result<(), OutputError> {
    fs::create_dir_all(path).map_err(|e| OutputError::CreateDir(e.to_string()))
}

fn list_existing_names(dir: &Path) -> HashSet<String> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn choose_suffix(existing: &HashSet<String>, base: &str, emit_pdf: bool, emit_jpeg: bool) -> u32 {
    for suffix in 1.. {
        let stem = if suffix == 1 {
            base.to_string()
        } else {
            format!("{base}_{suffix}")
        };
        let pdf_conflict = emit_pdf && existing.contains(&format!("{stem}.pdf"));
        let jpg_conflict = emit_jpeg && existing.contains(&format!("{stem}.jpg"));
        if !pdf_conflict && !jpg_conflict {
            return suffix;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn formats_date_folder() {
        assert_eq!(date_folder_name(NaiveDate::from_ymd_opt(2026, 7, 3).unwrap()), "20260703");
    }

    #[test]
    fn prefixes_and_sanitizes_stem() {
        let path = Path::new("/tmp/a:b?c.pdf");
        assert_eq!(prefixed_stem(path).unwrap(), "い）a_b_c");
    }

    #[test]
    fn plans_same_suffix_for_pdf_and_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 7, 3).unwrap();
        let date_dir = dir.path().join("20260703");
        fs::create_dir_all(&date_dir).unwrap();
        fs::write(date_dir.join("い）物件A.pdf"), b"exists").unwrap();

        let plan = plan_output_paths(dir.path(), Path::new("物件A.pdf"), true, true, date).unwrap();
        assert_eq!(plan.pdf_path.unwrap().file_name().unwrap(), "い）物件A_2.pdf");
        assert_eq!(plan.jpeg_path.unwrap().file_name().unwrap(), "い）物件A_2.jpg");
    }
}
