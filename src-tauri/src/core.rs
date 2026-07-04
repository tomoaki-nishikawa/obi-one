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

fn copy_clipped(dest: &mut RgbaImage, src: &RgbaImage, placed: Rect, clip: Rect) -> Rect {
    let placed_left = placed.x as i64;
    let placed_top = placed.y as i64;
    let placed_right = placed_left + placed.width as i64;
    let placed_bottom = placed_top + placed.height as i64;

    let clip_left = clip.x as i64;
    let clip_top = clip.y as i64;
    let clip_right = clip_left + clip.width as i64;
    let clip_bottom = clip_top + clip.height as i64;

    let copy_left = placed_left.max(clip_left);
    let copy_top = placed_top.max(clip_top);
    let copy_right = placed_right.min(clip_right);
    let copy_bottom = placed_bottom.min(clip_bottom);

    if copy_right <= copy_left || copy_bottom <= copy_top {
        return Rect {
            x: copy_left.max(0) as u32,
            y: copy_top.max(0) as u32,
            width: 0,
            height: 0,
        };
    }

    let src_x = (copy_left - placed_left) as u32;
    let src_y = (copy_top - placed_top) as u32;
    let copy_width = (copy_right - copy_left) as u32;
    let copy_height = (copy_bottom - copy_top) as u32;
    let visible = imageops::crop_imm(src, src_x, src_y, copy_width, copy_height).to_image();

    dest.copy_from(&visible, copy_left as u32, copy_top as u32)
        .expect("clipped image should fit destination");

    Rect {
        x: copy_left as u32,
        y: copy_top as u32,
        width: copy_width,
        height: copy_height,
    }
}

fn copy_region_clipped(dest: &mut RgbaImage, src: &RgbaImage, src_rect: Rect, dest_x: u32, dest_y: u32) -> Rect {
    let dest_width = dest.width();
    let dest_height = dest.height();
    if dest_x >= dest_width || dest_y >= dest_height || src_rect.width == 0 || src_rect.height == 0 {
        return Rect {
            x: dest_x.min(dest_width),
            y: dest_y.min(dest_height),
            width: 0,
            height: 0,
        };
    }

    let copy_width = src_rect.width.min(dest_width - dest_x);
    let copy_height = src_rect.height.min(dest_height - dest_y);
    let visible = imageops::crop_imm(src, src_rect.x, src_rect.y, copy_width, copy_height).to_image();

    dest.copy_from(&visible, dest_x, dest_y)
        .expect("clipped region should fit destination");

    Rect {
        x: dest_x,
        y: dest_y,
        width: copy_width,
        height: copy_height,
    }
}

fn property_visible_clip(placed: Rect, bounds: Rect, strip_obi: bool, obi_height_ratio: f32) -> Rect {
    if !strip_obi {
        return bounds;
    }

    let non_obi_height = ((placed.height as f32) * (1.0 - obi_height_ratio))
        .round()
        .max(1.0) as u32;
    let visible_bottom = (placed.y + non_obi_height).min(bounds.y + bounds.height);

    Rect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: visible_bottom.saturating_sub(bounds.y),
    }
}

pub fn composite_into_template(
    template: &DynamicImage,
    property: &DynamicImage,
    obi_band: Option<&DynamicImage>,
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
    let (property_width, property_height) = property_rgba.dimensions();
    let full_page = Rect {
        x: 0,
        y: 0,
        width: template_width,
        height: template_height,
    };
    let placed = if options.strip_obi && obi_band.is_some() {
        resize_to_fit(property_width, property_height, full_page)
    } else {
        resize_to_fit(property_width, property_height, area)
    };
    let resized = imageops::resize(&property_rgba, placed.width, placed.height, imageops::FilterType::Lanczos3);
    let property_clip = if options.strip_obi && obi_band.is_some() {
        property_visible_clip(placed, full_page, options.strip_obi, options.obi_cut_ratio)
    } else {
        property_visible_clip(placed, area, options.strip_obi, options.obi_cut_ratio)
    };

    let mut result = RgbaImage::from_pixel(template_width, template_height, Rgba([255, 255, 255, 255]));
    let copied_rect = if options.strip_obi {
        if let Some(obi_band) = obi_band {
            let copied_rect = copy_clipped(&mut result, &resized, placed, property_clip);
            let obi_top = copied_rect.y + copied_rect.height;
            let band_rgba = obi_band.to_rgba8();
            let band_height = ((band_rgba.height() as f32) * (template_width as f32 / band_rgba.width() as f32))
                .round()
                .max(1.0) as u32;
            let resized_band = imageops::resize(&band_rgba, template_width, band_height, imageops::FilterType::Lanczos3);
            copy_region_clipped(
                &mut result,
                &resized_band,
                Rect {
                    x: 0,
                    y: 0,
                    width: resized_band.width(),
                    height: resized_band.height(),
                },
                0,
                obi_top,
            );
            copied_rect
        } else {
        copy_region_clipped(
            &mut result,
            &template_rgba,
            Rect {
                x: 0,
                y: 0,
                width: template_width,
                height: template_obi_top,
            },
            0,
            0,
        );
        let copied_rect = copy_clipped(&mut result, &resized, placed, property_clip);
        let obi_top = copied_rect.y + copied_rect.height;
        copy_region_clipped(
            &mut result,
            &template_rgba,
            Rect {
                x: 0,
                y: template_obi_top,
                width: template_width,
                height: template_height.saturating_sub(template_obi_top),
            },
            0,
            obi_top,
        );
        copied_rect
        }
    } else {
        result.copy_from(&template_rgba, 0, 0).expect("template dimensions should match");
        copy_clipped(&mut result, &resized, placed, property_clip)
    };

    let info = CompositeInfo {
        template_obi_ratio: template_obi_top as f32 / template_height as f32,
        property_obi_ratio: (1.0 - options.obi_cut_ratio).clamp(0.0, 1.0),
        placed_rect: copied_rect,
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

        let (result, info) = composite_into_template(&template, &property, None, CompositeOptions::default()).unwrap();

        assert_eq!(result.dimensions(), (1000, 1400));
        assert!((info.property_obi_ratio - 0.8).abs() < 0.01);
        assert!(info.placed_rect.x < 1000);
        assert!(info.placed_rect.y < 1120);
    }

    #[test]
    fn lowering_obi_height_moves_visible_bottom_down_without_moving_property() {
        let mut template = solid(1000, 1400, [255, 255, 255, 255]).to_rgba8();
        for y in 1120..1400 {
            for x in 0..1000 {
                template.put_pixel(x, y, Rgba([20, 20, 20, 255]));
            }
        }
        let template = DynamicImage::ImageRgba8(template);
        let property = solid(800, 1000, [180, 180, 180, 255]);

        let (_, default_info) = composite_into_template(
            &template,
            &property,
            None,
            CompositeOptions {
                obi_cut_ratio: 0.20,
                ..CompositeOptions::default()
            },
        )
        .unwrap();
        let (_, lowered_info) = composite_into_template(
            &template,
            &property,
            None,
            CompositeOptions {
                obi_cut_ratio: 0.10,
                ..CompositeOptions::default()
            },
        )
        .unwrap();

        assert_eq!(lowered_info.placed_rect.y, default_info.placed_rect.y);
        assert_eq!(lowered_info.placed_rect.width, default_info.placed_rect.width);
        assert!(lowered_info.placed_rect.height >= default_info.placed_rect.height);
    }

    #[test]
    fn bundled_band_starts_at_declared_original_obi_top() {
        let template = solid(1000, 1000, [255, 255, 255, 255]);
        let property = solid(1000, 1000, [180, 180, 180, 255]);
        let band = solid(1000, 100, [20, 20, 20, 255]);

        let (result, info) = composite_into_template(
            &template,
            &property,
            Some(&band),
            CompositeOptions {
                obi_cut_ratio: 0.30,
                ..CompositeOptions::default()
            },
        )
        .unwrap();
        let result = result.to_rgba8();

        assert_eq!(info.placed_rect, Rect { x: 0, y: 0, width: 1000, height: 700 });
        assert_eq!(result.get_pixel(500, 699), &Rgba([180, 180, 180, 255]));
        assert_eq!(result.get_pixel(500, 700), &Rgba([20, 20, 20, 255]));
    }
}
