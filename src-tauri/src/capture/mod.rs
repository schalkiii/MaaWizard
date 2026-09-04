use std::path::Path;
use std::time::Duration;

use image::{ImageBuffer, Rgba, RgbaImage};
use maa_framework::buffer::MaaImageBuffer;

/// 抓取当前主显示器的桌面画面
pub fn capture_desktop() -> Result<RgbaImage, String> {
    let display = scrap::Display::primary().map_err(|e| e.to_string())?;
    let mut capturer = scrap::Capturer::new(display).map_err(|e| e.to_string())?;
    let width = capturer.width() as u32;
    let height = capturer.height() as u32;

    // 首帧可能尚未就绪，帧率未到时返回 WouldBlock，需要重试
    let frame: Vec<u8> = loop {
        match capturer.frame() {
            Ok(frame) => break frame.to_vec(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(16));
            }
            Err(error) => return Err(format!("截屏失败：{}", error)),
        }
    };

    // scrap 返回 BGRA，转换为 image crate 使用的 RGBA
    let mut image: RgbaImage = ImageBuffer::new(width, height);
    for (pixel, source) in image.pixels_mut().zip(frame.chunks_exact(4)) {
        pixel[0] = source[2];
        pixel[1] = source[1];
        pixel[2] = source[0];
        pixel[3] = 255;
    }

    Ok(image)
}

/// 以 (center_x, center_y) 为中心裁剪一块正方形 ROI，返回裁剪图与 [x, y, w, h]
pub fn crop_roi(
    image: &RgbaImage,
    center_x: i32,
    center_y: i32,
    size: u32,
) -> Option<(RgbaImage, [i32; 4])> {
    let (width, height) = image.dimensions();
    let half = (size / 2) as i32;

    let x = (center_x - half).clamp(0, width as i32 - 1);
    let y = (center_y - half).clamp(0, height as i32 - 1);
    let w = size.min(width.saturating_sub(x as u32));
    let h = size.min(height.saturating_sub(y as u32));

    if w == 0 || h == 0 {
        return None;
    }

    let cropped = image::imageops::crop_imm(image, x as u32, y as u32, w, h).to_image();
    Some((cropped, [x, y, w as i32, h as i32]))
}

/// 按任意矩形裁剪（供编辑器中拖拽框选 ROI 使用），返回裁剪图与 [x, y, w, h]
pub fn crop_rect(
    image: &RgbaImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<(RgbaImage, [i32; 4])> {
    let (image_width, image_height) = image.dimensions();
    let x = x.clamp(0, image_width as i32 - 1);
    let y = y.clamp(0, image_height as i32 - 1);
    let w = width.min(image_width.saturating_sub(x as u32));
    let h = height.min(image_height.saturating_sub(y as u32));

    if w == 0 || h == 0 {
        return None;
    }

    let cropped = image::imageops::crop_imm(image, x as u32, y as u32, w, h).to_image();
    Some((cropped, [x, y, w as i32, h as i32]))
}

/// 保存模板图片到资源包的 image 目录
pub fn save_template(image: &RgbaImage, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    image.save(path).map_err(|e| format!("保存模板失败：{}", e))
}

/// 把 MaaFramework 的 `MaaImageBuffer` 转成 PNG 字节（附带宽高）。
/// 优先用其编码数据（设备原始 PNG），编码不可用时回退到 raw(BGR(A)) 构造 RGBA 再编码，
/// 确保控制器截图（`cached_image()` 常无编码字段）一定能产出图。
pub fn maa_image_png_bytes(image: &MaaImageBuffer) -> Result<(Vec<u8>, u32, u32), String> {
    let width = image.width();
    let height = image.height();
    let channels = image.channels();
    if width <= 0 || height <= 0 || channels <= 0 {
        return Err("控制器截图无效（尺寸或通道数异常）".to_string());
    }
    if let Some(bytes) = image.to_vec().filter(|b| !b.is_empty()) {
        return Ok((bytes, width as u32, height as u32));
    }

    let raw = image.raw_data().ok_or("控制器截图为空，请确认已成功连接并截屏")?;
    let src_channels = channels as usize;
    let mut rgba: RgbaImage = ImageBuffer::new(width as u32, height as u32);
    for (pixel, chunk) in rgba.pixels_mut().zip(raw.chunks_exact(src_channels)) {
        *pixel = Rgba([
            chunk[2],
            chunk[1],
            chunk[0],
            if src_channels >= 4 { chunk[3] } else { 255 },
        ]);
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| format!("PNG 编码失败：{e}"))?;
    Ok((cursor.into_inner(), width as u32, height as u32))
}

/// 把 MaaFramework 的 `MaaImageBuffer` 存成 PNG。
pub fn save_maa_image(image: &MaaImageBuffer, path: &Path) -> Result<(), String> {
    let (bytes, _, _) = maa_image_png_bytes(image)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("保存截图失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn test_image(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([1, 2, 3, 255]))
    }

    #[test]
    fn crop_roi_center_produces_expected_rect() {
        let image = test_image(640, 480);
        let (cropped, rect) = crop_roi(&image, 320, 240, 80).unwrap();
        assert_eq!(rect, [280, 200, 80, 80]);
        assert_eq!(cropped.dimensions(), (80, 80));
    }

    #[test]
    fn crop_roi_clamps_to_image_bounds() {
        let image = test_image(640, 480);
        // 点击点贴近右下角时，裁剪框被钳制在边界内（起点 599，可延伸宽度仅剩 41）
        let (cropped, rect) = crop_roi(&image, 639, 479, 80).unwrap();
        assert_eq!(rect, [599, 439, 41, 41]);
        assert_eq!(cropped.dimensions(), (41, 41));
    }

    #[test]
    fn crop_roi_larger_than_image_returns_whole_image() {
        let image = test_image(640, 480);
        let (cropped, rect) = crop_roi(&image, 320, 240, 2000).unwrap();
        assert_eq!(rect, [0, 0, 640, 480]);
        assert_eq!(cropped.dimensions(), (640, 480));
    }

    #[test]
    fn crop_roi_zero_size_returns_none() {
        let image = test_image(100, 100);
        assert!(crop_roi(&image, 50, 50, 0).is_none());
    }

    #[test]
    fn crop_rect_out_of_bounds_is_clamped() {
        let image = test_image(200, 100);
        let (cropped, rect) = crop_rect(&image, 190, 90, 50, 50).unwrap();
        assert_eq!(rect, [190, 90, 10, 10]);
        assert_eq!(cropped.dimensions(), (10, 10));
    }

    #[test]
    fn crop_rect_negative_coordinates_clamp_to_zero() {
        let image = test_image(200, 100);
        let (_, rect) = crop_rect(&image, -20, -10, 50, 50).unwrap();
        assert_eq!(rect, [0, 0, 50, 50]);
    }

    #[test]
    fn save_template_creates_missing_directories() {
        let image = test_image(8, 8);
        let dir = std::env::temp_dir().join(format!("maa_wizard_test_{}", std::process::id()));
        let path = dir.join("nested/image/template.png");
        save_template(&image, &path).unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
