use image::imageops::FilterType;
use image::ImageFormat;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone, Serialize)]
pub struct ImageMeta {
    pub path: String,
    pub filename: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub format: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConvertResult {
    pub input_path: String,
    pub output_path: String,
    pub original_bytes: u64,
    pub output_bytes: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileIconZipResult {
    pub zip_path: String,
    pub total_files: usize,
    pub zip_bytes: u64,
}

pub fn get_image_metadata(path: &str) -> Result<ImageMeta, String> {
    let p = Path::new(path);
    let meta = std::fs::metadata(p).map_err(|e| format!("Cannot read file: {e}"))?;
    let (w, h) = image::image_dimensions(p).map_err(|e| format!("Invalid image file: {e}"))?;
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_uppercase();
    let filename = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    Ok(ImageMeta {
        path: path.to_string(),
        filename,
        width: w,
        height: h,
        size_bytes: meta.len(),
        format: ext,
    })
}

fn parse_target_format(fmt: &str, input_path: &Path) -> Result<(ImageFormat, String), String> {
    let lower = fmt.trim().to_ascii_lowercase();
    let resolved = if lower == "keep" || lower.is_empty() {
        input_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase()
    } else {
        lower
    };

    match resolved.as_str() {
        "png" => Ok((ImageFormat::Png, "png".into())),
        "jpg" | "jpeg" => Ok((ImageFormat::Jpeg, "jpg".into())),
        "webp" => Ok((ImageFormat::WebP, "webp".into())),
        "ico" => Ok((ImageFormat::Ico, "ico".into())),
        "bmp" => Ok((ImageFormat::Bmp, "bmp".into())),
        "gif" => Ok((ImageFormat::Gif, "gif".into())),
        other => Err(format!("Unsupported output format: {other}")),
    }
}

pub fn convert_and_process_images(
    input_paths: Vec<String>,
    target_format: String,
    resize_width: Option<u32>,
    resize_height: Option<u32>,
    keep_aspect: bool,
    quality: Option<u8>,
    output_dir: Option<String>,
) -> Result<Vec<ConvertResult>, String> {
    if input_paths.is_empty() {
        return Err("No input files selected.".into());
    }
    let q = quality.unwrap_or(85).clamp(10, 100);
    let mut results = Vec::with_capacity(input_paths.len());

    for raw_path in input_paths {
        let in_path = PathBuf::from(&raw_path);
        let original_bytes = std::fs::metadata(&in_path)
            .map_err(|e| format!("Failed to access '{}': {e}", raw_path))?
            .len();
        let mut img = image::open(&in_path)
            .map_err(|e| format!("Failed to decode image '{}': {e}", raw_path))?;

        let (fmt, ext) = parse_target_format(&target_format, &in_path)?;

        // Optional resize
        let (orig_w, orig_h) = (img.width(), img.height());
        let mut target_w = resize_width.filter(|&w| w > 0);
        let mut target_h = resize_height.filter(|&h| h > 0);

        if fmt == ImageFormat::Ico && target_w.is_none() && target_h.is_none() {
            if orig_w > 256 || orig_h > 256 {
                target_w = Some(256);
                target_h = Some(256);
            }
        }

        if target_w.is_some() || target_h.is_some() {
            let (rw, rh) = match (target_w, target_h) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => {
                    let h = ((orig_h as f64 / orig_w.max(1) as f64) * w as f64).round().max(1.0) as u32;
                    (w, h)
                }
                (None, Some(h)) => {
                    let w = ((orig_w as f64 / orig_h.max(1) as f64) * h as f64).round().max(1.0) as u32;
                    (w, h)
                }
                (None, None) => (orig_w, orig_h),
            };
            if fmt == ImageFormat::Ico {
                let cw = rw.min(256);
                let ch = rh.min(256);
                img = img.resize(cw, ch, FilterType::Lanczos3);
            } else if keep_aspect {
                img = img.resize(rw, rh, FilterType::Lanczos3);
            } else {
                img = img.resize_exact(rw, rh, FilterType::Lanczos3);
            }
        } else if fmt == ImageFormat::Ico && (orig_w > 256 || orig_h > 256) {
            img = img.resize(256, 256, FilterType::Lanczos3);
        }

        let out_parent = output_dir
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                in_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            });
        let _ = std::fs::create_dir_all(&out_parent);

        let stem = in_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let mut out_path = out_parent.join(format!("{stem}.{ext}"));
        if out_path == in_path {
            out_path = out_parent.join(format!("{stem}_raydrop.{ext}"));
        }

        let mut encoded = Vec::new();
        match fmt {
            ImageFormat::Jpeg => {
                let mut cursor = std::io::Cursor::new(&mut encoded);
                let rgb = img.to_rgb8();
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, q);
                encoder
                    .encode_image(&rgb)
                    .map_err(|e| format!("JPEG encode failed: {e}"))?;
            }
            ImageFormat::Png => {
                let mut cursor = std::io::Cursor::new(&mut encoded);
                let compression = if q < 95 {
                    image::codecs::png::CompressionType::Best
                } else {
                    image::codecs::png::CompressionType::Default
                };
                let encoder = image::codecs::png::PngEncoder::new_with_quality(
                    &mut cursor,
                    compression,
                    image::codecs::png::FilterType::Adaptive,
                );
                let rgba = img.to_rgba8();
                use image::ImageEncoder;
                encoder
                    .write_image(&rgba, rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
                    .map_err(|e| format!("PNG encode failed: {e}"))?;
            }
            _ => {
                let mut cursor = std::io::Cursor::new(&mut encoded);
                img.write_to(&mut cursor, fmt)
                    .map_err(|e| format!("Encode failed ({ext}): {e}"))?;
            }
        }

        std::fs::write(&out_path, &encoded)
            .map_err(|e| format!("Failed to write '{}': {e}", out_path.display()))?;

        results.push(ConvertResult {
            input_path: raw_path,
            output_path: out_path.to_string_lossy().to_string(),
            original_bytes,
            output_bytes: encoded.len() as u64,
            width: img.width(),
            height: img.height(),
        });
    }

    Ok(results)
}

pub fn generate_mobile_icons_zip(
    input_path: String,
    platform: String,
    output_zip_path: Option<String>,
) -> Result<MobileIconZipResult, String> {
    let in_path = Path::new(&input_path);
    let img = image::open(in_path)
        .map_err(|e| format!("Failed to decode source image '{}': {e}", input_path))?;

    let stem = in_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app_icon");
    let default_zip = in_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}_mobile_icons.zip"));
    let zip_path = output_zip_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_zip);

    let file = std::fs::File::create(&zip_path)
        .map_err(|e| format!("Failed to create zip '{}': {e}", zip_path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut total_files = 0usize;
    let plat = platform.to_ascii_lowercase();
    let include_android = plat == "both" || plat == "android";
    let include_ios = plat == "both" || plat == "ios";

    let mut write_resized_png = |entry_name: &str, size: u32| -> Result<(), String> {
        let resized = img.resize_exact(size, size, FilterType::Lanczos3);
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        resized
            .write_to(&mut cursor, ImageFormat::Png)
            .map_err(|e| format!("PNG encode failed for {entry_name}: {e}"))?;
        zip.start_file(entry_name, options)
            .map_err(|e| format!("Zip write error: {e}"))?;
        zip.write_all(&png_bytes)
            .map_err(|e| format!("Zip write error: {e}"))?;
        total_files += 1;
        Ok(())
    };

    if include_android {
        let android_sizes = [
            ("android/mipmap-mdpi/ic_launcher.png", 48u32),
            ("android/mipmap-mdpi/ic_launcher_round.png", 48),
            ("android/mipmap-hdpi/ic_launcher.png", 72),
            ("android/mipmap-hdpi/ic_launcher_round.png", 72),
            ("android/mipmap-xhdpi/ic_launcher.png", 96),
            ("android/mipmap-xhdpi/ic_launcher_round.png", 96),
            ("android/mipmap-xxhdpi/ic_launcher.png", 144),
            ("android/mipmap-xxhdpi/ic_launcher_round.png", 144),
            ("android/mipmap-xxxhdpi/ic_launcher.png", 192),
            ("android/mipmap-xxxhdpi/ic_launcher_round.png", 192),
            ("android/playstore-icon.png", 512),
        ];
        for (name, sz) in android_sizes {
            write_resized_png(name, sz)?;
        }
    }

    if include_ios {
        let ios_sizes = [
            ("ios/AppIcon.appiconset/Icon-20.png", 20u32),
            ("ios/AppIcon.appiconset/Icon-20@2x.png", 40),
            ("ios/AppIcon.appiconset/Icon-20@3x.png", 60),
            ("ios/AppIcon.appiconset/Icon-29.png", 29),
            ("ios/AppIcon.appiconset/Icon-29@2x.png", 58),
            ("ios/AppIcon.appiconset/Icon-29@3x.png", 87),
            ("ios/AppIcon.appiconset/Icon-40.png", 40),
            ("ios/AppIcon.appiconset/Icon-40@2x.png", 80),
            ("ios/AppIcon.appiconset/Icon-40@3x.png", 120),
            ("ios/AppIcon.appiconset/Icon-60@2x.png", 120),
            ("ios/AppIcon.appiconset/Icon-60@3x.png", 180),
            ("ios/AppIcon.appiconset/Icon-76.png", 76),
            ("ios/AppIcon.appiconset/Icon-76@2x.png", 152),
            ("ios/AppIcon.appiconset/Icon-83.5@2x.png", 167),
            ("ios/AppIcon.appiconset/Icon-1024.png", 1024),
        ];
        for (name, sz) in ios_sizes {
            write_resized_png(name, sz)?;
        }
        let contents_json = r#"{
  "images": [
    { "size": "20x20", "idiom": "iphone", "filename": "Icon-20@2x.png", "scale": "2x" },
    { "size": "20x20", "idiom": "iphone", "filename": "Icon-20@3x.png", "scale": "3x" },
    { "size": "29x29", "idiom": "iphone", "filename": "Icon-29@2x.png", "scale": "2x" },
    { "size": "29x29", "idiom": "iphone", "filename": "Icon-29@3x.png", "scale": "3x" },
    { "size": "40x40", "idiom": "iphone", "filename": "Icon-40@2x.png", "scale": "2x" },
    { "size": "40x40", "idiom": "iphone", "filename": "Icon-40@3x.png", "scale": "3x" },
    { "size": "60x60", "idiom": "iphone", "filename": "Icon-60@2x.png", "scale": "2x" },
    { "size": "60x60", "idiom": "iphone", "filename": "Icon-60@3x.png", "scale": "3x" },
    { "size": "20x20", "idiom": "ipad", "filename": "Icon-20.png", "scale": "1x" },
    { "size": "20x20", "idiom": "ipad", "filename": "Icon-20@2x.png", "scale": "2x" },
    { "size": "29x29", "idiom": "ipad", "filename": "Icon-29.png", "scale": "1x" },
    { "size": "29x29", "idiom": "ipad", "filename": "Icon-29@2x.png", "scale": "2x" },
    { "size": "40x40", "idiom": "ipad", "filename": "Icon-40.png", "scale": "1x" },
    { "size": "40x40", "idiom": "ipad", "filename": "Icon-40@2x.png", "scale": "2x" },
    { "size": "76x76", "idiom": "ipad", "filename": "Icon-76.png", "scale": "1x" },
    { "size": "76x76", "idiom": "ipad", "filename": "Icon-76@2x.png", "scale": "2x" },
    { "size": "83.5x83.5", "idiom": "ipad", "filename": "Icon-83.5@2x.png", "scale": "2x" },
    { "size": "1024x1024", "idiom": "ios-marketing", "filename": "Icon-1024.png", "scale": "1x" }
  ],
  "info": { "version": 1, "author": "raydrop" }
}"#;
        zip.start_file("ios/AppIcon.appiconset/Contents.json", options)
            .map_err(|e| format!("Zip write error: {e}"))?;
        zip.write_all(contents_json.as_bytes())
            .map_err(|e| format!("Zip write error: {e}"))?;
        total_files += 1;
    }

    zip.finish().map_err(|e| format!("Zip finish error: {e}"))?;
    let zip_bytes = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);

    Ok(MobileIconZipResult {
        zip_path: zip_path.to_string_lossy().to_string(),
        total_files,
        zip_bytes,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentEnginesInfo {
    pub has_word: bool,
    pub has_libreoffice: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentConvertResult {
    pub input_path: String,
    pub output_path: String,
    pub original_bytes: u64,
    pub output_bytes: u64,
    pub engine_used: String,
}

fn find_libreoffice_bin() -> Option<String> {
    let candidates: Vec<&str> = if cfg!(target_os = "windows") {
        vec![
            "soffice",
            r"C:\Program Files\LibreOffice\program\soffice.exe",
            r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "soffice",
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        ]
    } else {
        vec!["soffice", "/usr/bin/soffice", "/usr/local/bin/soffice"]
    };

    for c in candidates {
        if c == "soffice" || Path::new(c).exists() {
            let mut cmd = std::process::Command::new(c);
            cmd.arg("--version");
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            if let Ok(out) = cmd.output() {
                if out.status.success() {
                    return Some(c.to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn has_microsoft_word() -> bool {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .creation_flags(0x08000000)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "if (Test-Path 'HKLM:\\SOFTWARE\\Classes\\Word.Application') { exit 0 } else { exit 1 }",
        ])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

#[cfg(not(target_os = "windows"))]
fn has_microsoft_word() -> bool {
    false
}

pub fn detect_document_engines() -> DocumentEnginesInfo {
    DocumentEnginesInfo {
        has_word: has_microsoft_word(),
        has_libreoffice: find_libreoffice_bin().is_some(),
    }
}

#[cfg(target_os = "windows")]
fn convert_with_ms_word(in_path: &Path, out_path: &Path, target_fmt: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let in_escaped = in_path.to_string_lossy().replace('\'', "''");
    let out_escaped = out_path.to_string_lossy().replace('\'', "''");
    let save_call = if target_fmt == "pdf" {
        format!("$doc.ExportAsFixedFormat('{}', 17)", out_escaped)
    } else {
        // 16 = wdFormatXMLDocument (.docx)
        format!("$doc.SaveAs2('{}', 16)", out_escaped)
    };
    let script = format!(
        "$ErrorActionPreference = 'Stop';
         $word = New-Object -ComObject Word.Application;
         $word.Visible = $false;
         $word.DisplayAlerts = 0;
         try {{
           $doc = $word.Documents.Open('{}', $false, $true);
           {};
           $doc.Close($false);
         }} finally {{
           $word.Quit();
         }}",
        in_escaped, save_call
    );
    let out = std::process::Command::new("powershell")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("Failed to launch PowerShell for Word conversion: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("Microsoft Word conversion failed: {stderr}"));
    }
    if !out_path.exists() {
        return Err("Microsoft Word did not produce output file.".into());
    }
    Ok(())
}

fn convert_with_libreoffice(
    soffice: &str,
    in_path: &Path,
    out_dir: &Path,
    expected_out: &Path,
    target_fmt: &str,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new(soffice);
    cmd.arg("--headless");
    if target_fmt == "docx" {
        let ext = in_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "pdf" {
            cmd.arg("--infilter=writer_pdf_import");
        }
    }
    cmd.arg("--convert-to")
        .arg(target_fmt)
        .arg("--outdir")
        .arg(out_dir)
        .arg(in_path);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("Failed to run LibreOffice: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("LibreOffice conversion failed: {stderr}"));
    }
    if !expected_out.exists() {
        return Err("LibreOffice did not produce the expected output file.".into());
    }
    Ok(())
}

pub fn convert_documents(
    input_paths: Vec<String>,
    target_format: String,
    output_dir: Option<String>,
) -> Result<Vec<DocumentConvertResult>, String> {
    if input_paths.is_empty() {
        return Err("No document files selected.".into());
    }
    let target_fmt = target_format.trim().to_ascii_lowercase();
    if target_fmt != "pdf" && target_fmt != "docx" {
        return Err(format!("Unsupported target document format: {target_fmt}"));
    }

    let word_available = has_microsoft_word();
    let soffice_opt = find_libreoffice_bin();
    if !word_available && soffice_opt.is_none() {
        return Err(
            "Microsoft Word or LibreOffice is required for accurate DOC/DOCX ↔ PDF conversion. Please install LibreOffice (free) or Microsoft Word.".into(),
        );
    }

    let mut results = Vec::with_capacity(input_paths.len());
    for raw in input_paths {
        let in_path = PathBuf::from(&raw);
        let orig_bytes = std::fs::metadata(&in_path)
            .map_err(|e| format!("Cannot read '{}': {e}", raw))?
            .len();
        let out_parent = output_dir
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                in_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            });
        let _ = std::fs::create_dir_all(&out_parent);

        let stem = in_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let mut out_path = out_parent.join(format!("{stem}.{target_fmt}"));
        if out_path == in_path {
            out_path = out_parent.join(format!("{stem}_converted.{target_fmt}"));
        }

        let engine_used: String;
        #[cfg(target_os = "windows")]
        if word_available {
            match convert_with_ms_word(&in_path, &out_path, &target_fmt) {
                Ok(_) => {
                    engine_used = "Microsoft Word".to_string();
                    let out_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                    results.push(DocumentConvertResult {
                        input_path: raw,
                        output_path: out_path.to_string_lossy().to_string(),
                        original_bytes: orig_bytes,
                        output_bytes: out_bytes,
                        engine_used,
                    });
                    continue;
                }
                Err(word_err) => {
                    if let Some(ref soffice) = soffice_opt {
                        convert_with_libreoffice(soffice, &in_path, &out_parent, &out_path, &target_fmt)?;
                        engine_used = "LibreOffice".to_string();
                        let out_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                        results.push(DocumentConvertResult {
                            input_path: raw,
                            output_path: out_path.to_string_lossy().to_string(),
                            original_bytes: orig_bytes,
                            output_bytes: out_bytes,
                            engine_used,
                        });
                        continue;
                    }
                    return Err(word_err);
                }
            }
        }

        if let Some(ref soffice) = soffice_opt {
            convert_with_libreoffice(soffice, &in_path, &out_parent, &out_path, &target_fmt)?;
            engine_used = "LibreOffice".to_string();
        } else {
            return Err("No supported Office/LibreOffice converter engine found.".into());
        }

        let out_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        results.push(DocumentConvertResult {
            input_path: raw,
            output_path: out_path.to_string_lossy().to_string(),
            original_bytes: orig_bytes,
            output_bytes: out_bytes,
            engine_used,
        });
    }

    Ok(results)
}
