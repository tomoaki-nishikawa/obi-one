use image::{imageops, DynamicImage, GenericImage, GenericImageView, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("帯カット率は5%から40%の間で指定してください。")]
    InvalidCutRatio,
    #[error("テンプレートの挿入領域を計算できませんでした。")]
    InvalidInsertArea,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CompositeOptions {
    pub strip_obi: bool,
    pub obi_cut_ratio: f32,
    pub margin_ratio: f32,
}

impl Default for CompositeOptions {
    fn default() -> Self {
        Self {
            strip_obi: true,
            obi_cut_ratio: 0.20,
            margin_ratio: 0.018,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompositeInfo {
    pub template_obi_ratio: f32,
    pub property_obi_ratio: f32,
    pub placed_rect: Rect,
}

pub fn clamp_cut_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.05, 0.40)
}

pub fn validate_cut_ratio(ratio: f32) -> Result<(), CoreError> {
    if (0.05..=0.40).contains(&ratio) {
        Ok(())
    } else {
        Err(CoreError::InvalidCutRatio)
    }
}

pub fn detect_obi_top_by_luminance(img: &DynamicImage, search_ratio: f32) -> u32 {
    let gray = img.to_luma8();
    let (width, height) = gray.dimensions();
    if width == 0 || height < 3 {
        return (height as f32 * 0.80).round() as u32;
    }

    let search_start = ((height as f32) * (1.0 - search_ratio.clamp(0.05, 0.90))).round() as u32;
    let mut best_y = search_start;
    let mut best_strength: u64 = 0;

    for y in search_start.max(1)..height.saturating_sub(1) {
        let mut strength = 0u64;
        for x in 0..width {
            let above = gray.get_pixel(x, y - 1)[0] as i16;
            let below = gray.get_pixel(x, y + 1)[0] as i16;
            strength += (below - above).unsigned_abs() as u64;
        }
        if strength > best_strength {
            best_strength = strength;
            best_y = y;
        }
    }

    if best_strength == 0 {
        (height as f32 * 0.80).round() as u32
    } else {
        best_y
    }
}

pub fn insert_area(template_width: u32, template_height: u32, obi_top: u32, margin_ratio: f32) -> Result<Rect, CoreError> {
    let margin_x = ((template_width as f32) * margin_ratio).round() as u32;
    let margin_y = ((template_height as f32) * margin_ratio).round() as u32;

    let x = margin_x;
    let y = margin_y;
    let right = template_width.saturating_sub(margin_x);
    let bottom = obi_top.saturating_sub(margin_y);

    if right <= x || bottom <= y {
        return Err(CoreError::InvalidInsertArea);
    }

    Ok(Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

pub fn resize_to_fit(src_width: u32, src_height: u32, bounds: Rect) -> Rect {
    let scale = (bounds.width as f32 / src_width as f32).min(bounds.height as f32 / src_height as f32);
    let width = ((src_width as f32) * scale).round().max(1.0) as u32;
    let height = ((src_height as f32) * scale).round().max(1.0) as u32;

    Rect {
        x: bounds.x + (bounds.width - width) / 2,
        y: bounds.y + (bounds.height - height) / 2,
        width,
        height,
    }
}

pub fn composite_into_template(
    template: &DynamicImage,
    property: &DynamicImage,
    options: CompositeOptions,
) -> Result<(DynamicImage, CompositeInfo), CoreError> {
    validate_cut_ratio(options.obi_cut_ratio)?;

    let template_rgba = template.to_rgba8();
    let template_img = DynamicImage::ImageRgba8(template_rgba.clone());
    let (template_width, template_height) = template_img.dimensions();

    let template_obi_top = detect_obi_top_by_luminance(&template_img, 0.45);
    let area = insert_area(
        template_width,
        template_height,
        template_obi_top,
        options.margin_ratio,
    )?;

    let property_rgba = property.to_rgba8();
    let (_, property_height) = property_rgba.dimensions();
    let crop_height = if options.strip_obi {
        ((property_height as f32) * (1.0 - options.obi_cut_ratio))
            .round()
            .max((property_height / 2) as f32) as u32
    } else {
        property_height
    };

    let cropped = imageops::crop_imm(&property_rgba, 0, 0, property_rgba.width(), crop_height).to_image();
    let placed = resize_to_fit(cropped.width(), cropped.height(), area);
    let resized = imageops::resize(&cropped, placed.width, placed.height, imageops::FilterType::Lanczos3);

    let mut result = RgbaImage::from_pixel(template_width, template_height, Rgba([255, 255, 255, 255]));
    result.copy_from(&template_rgba, 0, 0).expect("template dimensions should match");
    result.copy_from(&resized, placed.x, placed.y).expect("placed image should fit");

    let info = CompositeInfo {
        template_obi_ratio: template_obi_top as f32 / template_height as f32,
        property_obi_ratio: crop_height as f32 / property_height as f32,
        placed_rect: placed,
    };

    Ok((DynamicImage::ImageRgba8(result), info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba(rgba)))
    }

    #[test]
    fn validates_cut_ratio() {
        assert!(validate_cut_ratio(0.20).is_ok());
        assert!(validate_cut_ratio(0.04).is_err());
        assert!(validate_cut_ratio(0.41).is_err());
    }

    #[test]
    fn computes_insert_area_with_margin() {
        let rect = insert_area(1000, 1400, 1120, 0.018).unwrap();
        assert_eq!(rect, Rect { x: 18, y: 25, width: 964, height: 1070 });
    }

    #[test]
    fn resizes_to_fit_without_changing_aspect_ratio() {
        let bounds = Rect { x: 10, y: 20, width: 500, height: 300 };
        let placed = resize_to_fit(1000, 1000, bounds);
        assert_eq!(placed, Rect { x: 110, y: 20, width: 300, height: 300 });
    }

    #[test]
    fn composite_keeps_template_size_and_cuts_property() {
        let mut template = solid(1000, 1400, [255, 255, 255, 255]).to_rgba8();
        for y in 1120..1400 {
            for x in 0..1000 {
                template.put_pixel(x, y, Rgba([20, 20, 20, 255]));
            }
        }
        let template = DynamicImage::ImageRgba8(template);
        let property = solid(800, 1000, [180, 180, 180, 255]);

        let (result, info) = composite_into_template(&template, &property, CompositeOptions::default()).unwrap();

        assert_eq!(result.dimensions(), (1000, 1400));
        assert!((info.property_obi_ratio - 0.8).abs() < 0.01);
        assert!(info.placed_rect.x < 1000);
        assert!(info.placed_rect.y < 1120);
    }
}
