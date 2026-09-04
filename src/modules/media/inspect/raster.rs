use std::io::Cursor;

use bytes::Bytes;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{AnimationDecoder, ExtendedColorType, ImageDecoder, ImageEncoder};

use crate::error::AppError;
use crate::modules::media::inspect::{
    CleanMedia, as_i32_dim, invalid, need, u16_be, u16_le, u32_be, u32_le, validate_dims,
};

pub fn png(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let (width, height) = png_ihdr_dims(bytes)?;
    validate_dims(width, height)?;
    reencode_png(bytes, width, height)
}

pub fn jpeg(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    if let Some((width, height)) = jpeg_sof_dims(bytes)? {
        validate_dims(width, height)?;
    }
    let img = image::load_from_memory(bytes).map_err(|_| invalid("jpeg payload"))?;
    validate_dims(img.width(), img.height())?;
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, 90);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|_| invalid("jpeg rewrite"))?;
    Ok(CleanMedia {
        bytes: Bytes::from(out),
        mime: "image/jpeg".into(),
        extension: "jpg",
        width: as_i32_dim(rgb.width()),
        height: as_i32_dim(rgb.height()),
    })
}

pub fn gif(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let (width, height) = gif_screen_dims(bytes)?;
    validate_dims(width, height)?;
    let stripped = strip_gif(bytes)?;
    let decoder = GifDecoder::new(Cursor::new(&stripped)).map_err(|_| invalid("gif payload"))?;
    let (dw, dh) = decoder.dimensions();
    validate_dims(dw, dh)?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|_| invalid("gif frames"))?;
    if frames.is_empty() {
        return Err(invalid("gif has no frames"));
    }
    for frame in &frames {
        let buffer = frame.buffer();
        validate_dims(buffer.width(), buffer.height())?;
    }
    Ok(CleanMedia {
        bytes: Bytes::from(stripped),
        mime: "image/gif".into(),
        extension: "gif",
        width: as_i32_dim(dw),
        height: as_i32_dim(dh),
    })
}

pub fn webp(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let stripped = strip_riff_webp(bytes)?;
    if let Some((width, height)) = webp_canvas_dims(&stripped)? {
        validate_dims(width, height)?;
        return reencode_webp(&stripped, width, height);
    }
    reencode_webp(&stripped, 0, 0)
}

fn reencode_webp(bytes: &[u8], header_w: u32, header_h: u32) -> Result<CleanMedia, AppError> {
    let img = image::load_from_memory(bytes).map_err(|_| invalid("image payload"))?;
    if header_w != 0 && (img.width() != header_w || img.height() != header_h) {
        return Err(invalid("image header does not match pixels"));
    }
    validate_dims(img.width(), img.height())?;
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    WebPEncoder::new_lossless(&mut out)
        .encode(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| invalid("webp rewrite"))?;
    Ok(CleanMedia {
        bytes: Bytes::from(out),
        mime: "image/webp".into(),
        extension: "webp",
        width: as_i32_dim(rgba.width()),
        height: as_i32_dim(rgba.height()),
    })
}

fn reencode_png(bytes: &[u8], header_w: u32, header_h: u32) -> Result<CleanMedia, AppError> {
    let img = image::load_from_memory(bytes).map_err(|_| invalid("image payload"))?;
    if header_w != 0 && (img.width() != header_w || img.height() != header_h) {
        return Err(invalid("image header does not match pixels"));
    }
    validate_dims(img.width(), img.height())?;
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| invalid("png rewrite"))?;
    Ok(CleanMedia {
        bytes: Bytes::from(out),
        mime: "image/png".into(),
        extension: "png",
        width: as_i32_dim(rgba.width()),
        height: as_i32_dim(rgba.height()),
    })
}

fn png_ihdr_dims(bytes: &[u8]) -> Result<(u32, u32), AppError> {
    if need(bytes, 12, 4)? != b"IHDR" {
        return Err(invalid("png header"));
    }
    Ok((u32_be(bytes, 16)?, u32_be(bytes, 20)?))
}

fn gif_screen_dims(bytes: &[u8]) -> Result<(u32, u32), AppError> {
    Ok((u32::from(u16_le(bytes, 6)?), u32::from(u16_le(bytes, 8)?)))
}

fn jpeg_sof_dims(bytes: &[u8]) -> Result<Option<(u32, u32)>, AppError> {
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return Err(invalid("jpeg marker"));
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if i + 2 > bytes.len() {
            return Err(invalid("jpeg marker length"));
        }
        let len = usize::from(u16_be(bytes, i)?);
        if len < 2 || i + len > bytes.len() {
            return Err(invalid("jpeg marker length"));
        }
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) && len >= 7 {
            let height = u32::from(u16_be(bytes, i + 3)?);
            let width = u32::from(u16_be(bytes, i + 5)?);
            return Ok(Some((width, height)));
        }
        i += len;
    }
    Ok(None)
}

fn strip_gif(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    if bytes.len() < 13 {
        return Err(invalid("gif header"));
    }
    let packed = bytes[10];
    let mut i: usize = 13;
    if packed & 0x80 != 0 {
        let table = 3 * (2_usize.pow(u32::from(packed & 7) + 1));
        i = i.checked_add(table).ok_or_else(|| invalid("gif table"))?;
    }
    if i > bytes.len() {
        return Err(invalid("gif table"));
    }
    let mut out = bytes[..i].to_vec();
    while i < bytes.len() {
        match bytes[i] {
            0x3B => {
                out.push(0x3B);
                if i + 1 != bytes.len() {
                    return Err(invalid("trailing data after gif"));
                }
                return Ok(out);
            }
            0x2C => {
                let start = i;
                i = i.checked_add(10).ok_or_else(|| invalid("gif image"))?;
                if i > bytes.len() {
                    return Err(invalid("gif image"));
                }
                let local = bytes[start + 9];
                if local & 0x80 != 0 {
                    let table = 3 * (2_usize.pow(u32::from(local & 7) + 1));
                    i = i.checked_add(table).ok_or_else(|| invalid("gif table"))?;
                }
                i = skip_gif_subblocks(bytes, i.checked_add(1).ok_or_else(|| invalid("gif lzw"))?)?;
                out.extend_from_slice(&bytes[start..i]);
            }
            0x21 => {
                if i + 2 > bytes.len() {
                    return Err(invalid("gif extension"));
                }
                let label = bytes[i + 1];
                let start = i;
                i = skip_gif_subblocks(bytes, i + 2)?;
                match label {
                    0x01 => return Err(invalid("gif plain-text extension")),
                    0xFE => {}
                    _ => out.extend_from_slice(&bytes[start..i]),
                }
            }
            _ => return Err(invalid("gif block")),
        }
    }
    Err(invalid("gif trailer"))
}

fn skip_gif_subblocks(bytes: &[u8], mut i: usize) -> Result<usize, AppError> {
    loop {
        let size = usize::from(*bytes.get(i).ok_or_else(|| invalid("gif sub-block"))?);
        i = i.checked_add(1).ok_or_else(|| invalid("gif sub-block"))?;
        if size == 0 {
            return Ok(i);
        }
        i = i
            .checked_add(size)
            .ok_or_else(|| invalid("gif sub-block"))?;
        if i > bytes.len() {
            return Err(invalid("gif sub-block"));
        }
    }
}

fn strip_riff_webp(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    if bytes.len() < 12 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
        return Err(invalid("webp header"));
    }
    let declared = u32_le(bytes, 4)? as usize;
    let end = declared
        .checked_add(8)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| invalid("webp size"))?;
    let mut i = 12;
    let mut chunks = Vec::new();
    while i + 8 <= end {
        let fourcc = need(bytes, i, 4)?;
        let size = u32_le(bytes, i + 4)? as usize;
        let data_start = i + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| invalid("webp chunk"))?;
        if data_end > end {
            return Err(invalid("webp chunk"));
        }
        let padded = data_end + usize::from(size % 2 == 1);
        if matches!(fourcc, b"ANIM" | b"ANMF") {
            return Err(invalid("animated webp"));
        }
        if matches!(fourcc, b"EXIF" | b"XMP " | b"ICCP") {
            i = padded.min(end);
            continue;
        }
        if !matches!(fourcc, b"VP8 " | b"VP8L" | b"VP8X" | b"ALPH") {
            return Err(invalid("webp chunk"));
        }
        chunks.extend_from_slice(&bytes[i..data_end]);
        if size % 2 == 1 {
            chunks.push(0);
        }
        i = padded.min(end);
    }
    if i != end && i < bytes.len() && end != bytes.len() {
        return Err(invalid("trailing data after webp"));
    }
    if chunks.is_empty() {
        return Err(invalid("webp payload"));
    }
    let mut out = Vec::with_capacity(12 + chunks.len());
    out.extend_from_slice(b"RIFF");
    let riff_size = u32::try_from(chunks.len() + 4).map_err(|_| invalid("webp size"))?;
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&chunks);
    Ok(out)
}

fn webp_canvas_dims(bytes: &[u8]) -> Result<Option<(u32, u32)>, AppError> {
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let fourcc = need(bytes, i, 4)?;
        let size = u32_le(bytes, i + 4)? as usize;
        let data = i + 8;
        if fourcc == b"VP8X" && size >= 10 && data + 10 <= bytes.len() {
            let width = 1
                + u32::from(bytes[data + 4])
                + (u32::from(bytes[data + 5]) << 8)
                + (u32::from(bytes[data + 6]) << 16);
            let height = 1
                + u32::from(bytes[data + 7])
                + (u32::from(bytes[data + 8]) << 8)
                + (u32::from(bytes[data + 9]) << 16);
            return Ok(Some((width, height)));
        }
        i = data
            .checked_add(size + usize::from(size % 2 == 1))
            .ok_or_else(|| invalid("webp chunk"))?;
    }
    Ok(None)
}
