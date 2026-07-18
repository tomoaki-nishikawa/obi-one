use crate::core::{composite_into_template, CompositeInfo, CompositeOptions};
use crate::output::{ensure_output_dir, plan_output_paths, OutputError};
use crate::settings::{AppSettings, TemplateMode};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use image::{codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView};
use pdfium_render::prelude::*;
use printpdf::{Image as PdfImage, ImageTransform, Mm, PdfDocument as PrintPdfDocument};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Cursor};
use std::path::{Path, PathBuf};
use tauri::{path::BaseDirectory, AppHandle, Manager};

const RENDER_DPI: f32 = 150.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub input_path: String,
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,
    pub pdf_path: Option<String>,
    pub jpeg_path: Option<String>,
    pub output_dir: String,
    pub info: CompositeInfo,
}

pub fn process_pdf(app: &AppHandle, request: ProcessRequest) -> Result<ProcessResult> {
    let input_path = PathBuf::from(&request.input_path);
    validate_input_pdf(&input_path)?;

    let output_root = request
        .settings
        .output_dir
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("出力先フォルダを選択してください。"))?;

    if !request.settings.emit_pdf && !request.settings.emit_jpeg {
        return Err(anyhow!("PDFまたはJPEGのどちらかは出力してください。"));
    }

    let template_path = resolve_template_path(app)?;
    let plan = plan_output_paths(
        &output_root,
        &input_path,
        request.settings.emit_pdf,
        request.settings.emit_jpeg,
        Local::now().date_naive(),
    )
    .map_err(output_error_to_anyhow)?;
    ensure_output_dir(&plan.date_dir).map_err(output_error_to_anyhow)?;

    let pdfium = pdfium(app)?;

    let template_img = render_pdf_first_page(&pdfium, &template_path)
        .with_context(|| format!("テンプレートPDFを開けませんでした: {}", template_path.display()))?;
    let obi_band_path = resolve_obi_band_path(app, &request.settings)?;
    let obi_band_img = image::open(&obi_band_path)
        .with_context(|| format!("帯画像を開けませんでした: {}", obi_band_path.display()))?;
    let property_img = render_single_page_property_pdf(&pdfium, &input_path)
        .with_context(|| format!("物件PDFを開けませんでした: {}", input_path.display()))?;

    let options = CompositeOptions {
        strip_obi: true,
        obi_cut_ratio: request.settings.obi_cut_ratio,
        ..CompositeOptions::default()
    };
    let (composited, info) = composite_into_template(&template_img, &property_img, Some(&obi_band_img), options)
        .map_err(|e| anyhow!(e.to_string()))?;

    save_processed_outputs(
        &property_img,
        &composited,
        plan.jpeg_path.as_deref(),
        plan.pdf_path.as_deref(),
    )?;

    Ok(ProcessResult {
        input_path: request.input_path,
        pdf_path: plan.pdf_path.map(path_to_string),
        jpeg_path: plan.jpeg_path.map(path_to_string),
        output_dir: path_to_string(plan.date_dir),
        info,
    })
}

fn resolve_obi_band_path(app: &AppHandle, settings: &AppSettings) -> Result<PathBuf> {
    if settings.template_mode == TemplateMode::Custom {
        let path = settings
            .custom_template_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("カスタム帯画像が未指定です。"))?;
        if path.exists() {
            return Ok(path);
        }
        return Err(anyhow!("カスタム帯画像が見つかりません: {}", path.display()));
    }

    let mut candidates = Vec::new();
    if let Ok(path) = app.path().resolve("maisoku_obi_band.jpg", BaseDirectory::Resource) {
        candidates.push(path);
    }
    if let Ok(path) = app.path().resolve("resources/maisoku_obi_band.jpg", BaseDirectory::Resource) {
        candidates.push(path);
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("maisoku_obi_band.jpg"));

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "帯画像が見つかりませんでした。探索先:\n{}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

fn validate_input_pdf(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("入力PDFが見つかりません: {}", path.display()));
    }
    if path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.eq_ignore_ascii_case("pdf")) != Some(true) {
        return Err(anyhow!("PDFファイルを選択してください: {}", path.display()));
    }
    Ok(())
}

fn resolve_template_path(app: &AppHandle) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = app.path().resolve("maisoku_template.pdf", BaseDirectory::Resource) {
        candidates.push(path);
    }
    if let Ok(path) = app.path().resolve("resources/maisoku_template.pdf", BaseDirectory::Resource) {
        candidates.push(path);
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("maisoku_template.pdf"));

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "プリセットPDFが見つかりませんでした。探索先:\n{}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

fn pdfium(app: &AppHandle) -> Result<Pdfium> {
    let library_name = pdfium_library_name();
    let mut candidates = Vec::new();

    if let Ok(path) = app.path().resolve(format!("pdfium/{library_name}"), BaseDirectory::Resource) {
        candidates.push(path);
    }
    if let Ok(path) = app.path().resolve(format!("resources/pdfium/{library_name}"), BaseDirectory::Resource) {
        candidates.push(path);
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("pdfium").join(library_name));
    candidates.push(Pdfium::pdfium_platform_library_name_at_path("./"));

    let mut errors = Vec::new();
    for candidate in candidates {
        match Pdfium::bind_to_library(&candidate) {
            Ok(bindings) => return Ok(Pdfium::new(bindings)),
            Err(error) => errors.push(format!("{}: {}", candidate.display(), error)),
        }
    }

    let bindings = Pdfium::bind_to_system_library()
        .map_err(|error| {
            anyhow!(
                "PDFiumライブラリを読み込めませんでした。同梱PDFiumを配置するには `./scripts/fetch-pdfium.sh` を実行してください。\n探索結果:\n{}\nシステム探索: {}",
                errors.join("\n"),
                error
            )
        })?;
    Ok(Pdfium::new(bindings))
}

fn pdfium_library_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "pdfium.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libpdfium.dylib"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "libpdfium.so"
    }
}

fn render_pdf_first_page(pdfium: &Pdfium, path: &Path) -> Result<DynamicImage> {
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| anyhow!("PDFを読み込めませんでした: {e}"))?;
    render_first_page_from_doc(&doc)
}

fn render_single_page_property_pdf(pdfium: &Pdfium, path: &Path) -> Result<DynamicImage> {
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| anyhow!("PDFを読み込めませんでした: {e}"))?;
    if doc.pages().len() != 1 {
        return Err(anyhow!("物件PDFは1ページのみ対応です。現在のページ数: {}", doc.pages().len()));
    }
    render_first_page_from_doc(&doc)
}

fn render_first_page_from_doc(doc: &PdfDocument) -> Result<DynamicImage> {
    if doc.pages().is_empty() {
        return Err(anyhow!("PDFにページがありません。"));
    }
    let page = doc.pages().get(0).map_err(|e| anyhow!("PDFの1ページ目を開けませんでした: {e}"))?;
    let width = ((page.width().value / 72.0) * RENDER_DPI).round() as i32;
    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(width)
                .render_form_data(true),
        )
        .map_err(|e| anyhow!("PDFを画像化できませんでした: {e}"))?;
    Ok(bitmap.as_image())
}

fn save_jpeg(img: &DynamicImage, path: &Path) -> Result<()> {
    let file = File::create(path).with_context(|| format!("JPEGを書き込めませんでした: {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let rgb = img.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
    encoder
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .with_context(|| format!("JPEGを保存できませんでした: {}", path.display()))
}

fn save_processed_outputs(
    original_property: &DynamicImage,
    composited: &DynamicImage,
    jpeg_path: Option<&Path>,
    pdf_path: Option<&Path>,
) -> Result<()> {
    if let Some(path) = jpeg_path {
        save_jpeg(original_property, path)?;
    }
    if let Some(path) = pdf_path {
        save_image_as_pdf(composited, path)?;
    }
    Ok(())
}

fn save_image_as_pdf(img: &DynamicImage, path: &Path) -> Result<()> {
    let (width, height) = img.dimensions();
    let width_mm = pixels_to_mm(width);
    let height_mm = pixels_to_mm(height);
    let (doc, page, layer) = PrintPdfDocument::new("obi-one output", Mm(width_mm), Mm(height_mm), "Layer 1");
    let layer = doc.get_page(page).get_layer(layer);

    let mut jpeg = Cursor::new(Vec::new());
    let rgb = img.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 95);
    encoder
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .context("PDF埋め込み用画像を作成できませんでした。")?;
    let pdf_dynamic = printpdf::image_crate::load_from_memory(&jpeg.into_inner())
        .context("PDF埋め込み用画像を読み込めませんでした。")?;
    let pdf_image = PdfImage::from_dynamic_image(&pdf_dynamic);
    pdf_image.add_to_layer(
        layer,
        ImageTransform {
            translate_x: Some(Mm(0.0)),
            translate_y: Some(Mm(0.0)),
            rotate: None,
            scale_x: Some(1.0),
            scale_y: Some(1.0),
            dpi: Some(RENDER_DPI),
        },
    );

    let file = File::create(path).with_context(|| format!("PDFを書き込めませんでした: {}", path.display()))?;
    doc.save(&mut BufWriter::new(file))
        .with_context(|| format!("PDFを保存できませんでした: {}", path.display()))
}

fn pixels_to_mm(px: u32) -> f32 {
    px as f32 / RENDER_DPI * 25.4
}

fn output_error_to_anyhow(error: OutputError) -> anyhow::Error {
    anyhow!(error.to_string())
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pdfium_renders_bundled_template_pdf() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let pdfium_path = manifest_dir
            .join("resources")
            .join("pdfium")
            .join(pdfium_library_name());
        let template_path = manifest_dir
            .join("resources")
            .join("maisoku_template.pdf");

        assert!(pdfium_path.exists(), "missing bundled PDFium: {}", pdfium_path.display());
        assert!(template_path.exists(), "missing bundled template: {}", template_path.display());

        let bindings = Pdfium::bind_to_library(&pdfium_path).expect("bundled PDFium should load");
        let pdfium = Pdfium::new(bindings);
        let image = render_pdf_first_page(&pdfium, &template_path).expect("template should render");

        assert!(image.width() > 0);
        assert!(image.height() > 0);
    }

    #[test]
    fn can_save_rendered_template_as_pdf_and_jpeg() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let pdfium_path = manifest_dir
            .join("resources")
            .join("pdfium")
            .join(pdfium_library_name());
        let template_path = manifest_dir
            .join("resources")
            .join("maisoku_template.pdf");
        let out_dir = tempfile::tempdir().unwrap();
        let jpeg_path = out_dir.path().join("out.jpg");
        let pdf_path = out_dir.path().join("out.pdf");

        let bindings = Pdfium::bind_to_library(&pdfium_path).expect("bundled PDFium should load");
        let pdfium = Pdfium::new(bindings);
        let image = render_pdf_first_page(&pdfium, &template_path).expect("template should render");

        save_jpeg(&image, &jpeg_path).expect("jpeg should save");
        save_image_as_pdf(&image, &pdf_path).expect("pdf should save");

        assert!(jpeg_path.metadata().unwrap().len() > 0);
        assert!(pdf_path.metadata().unwrap().len() > 0);

        let rendered_pdf = render_pdf_first_page(&pdfium, &pdf_path).expect("saved pdf should render");
        assert!(non_white_pixel_count(&rendered_pdf) > 1000, "saved pdf should not be blank");
    }

    #[test]
    fn jpeg_output_uses_original_property_before_compositing() {
        let out_dir = tempfile::tempdir().unwrap();
        let jpeg_path = out_dir.path().join("original.jpg");
        let original = DynamicImage::new_rgb8(8, 8);
        let composited = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            8,
            image::Rgb([255, 255, 255]),
        ));

        save_processed_outputs(&original, &composited, Some(&jpeg_path), None)
            .expect("jpeg should save");

        let saved = image::open(jpeg_path).expect("saved jpeg should open").to_rgb8();
        let pixel = saved.get_pixel(0, 0);
        assert!(pixel[0] < 10 && pixel[1] < 10 && pixel[2] < 10);
    }

    fn non_white_pixel_count(image: &DynamicImage) -> usize {
        image
            .to_rgb8()
            .pixels()
            .filter(|pixel| pixel[0] < 245 || pixel[1] < 245 || pixel[2] < 245)
            .count()
    }
}
