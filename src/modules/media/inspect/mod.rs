mod audio;
mod container;
mod raster;

use std::path::Path;

use bytes::Bytes;

use crate::error::AppError;
use crate::modules::media::kinds::MediaKind;

const MAX_EDGE: u32 = 16_384;
const MAX_PIXELS: u64 = 48_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sniffed {
    Png,
    Jpeg,
    Gif,
    Webp,
    Wav,
    Mp3,
    Ogg,
    Flac,
    Aac,
    Mp4,
    Webm,
    QuickTime,
}

pub struct CleanMedia {
    pub bytes: Bytes,
    pub mime: String,
    pub extension: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

pub fn sanitize(
    kind: MediaKind,
    filename: &str,
    declared_mime: &str,
    bytes: &[u8],
) -> Result<CleanMedia, AppError> {
    if bytes.is_empty() {
        return Err(invalid("file is empty"));
    }
    let Some(sniffed) = sniff(bytes) else {
        reject_hostile_prefix(bytes)?;
        return Err(invalid("unrecognized media type"));
    };
    if declared_conflicts(declared_mime, sniffed) {
        return Err(invalid("declared type does not match contents"));
    }
    if !kind_allows(kind, sniffed) {
        return Err(invalid(&format!(
            "file contents are not a valid {}",
            kind.as_str()
        )));
    }
    if extension_conflicts(filename, sniffed) {
        return Err(invalid("file extension does not match contents"));
    }
    let clean = match sniffed {
        Sniffed::Png => raster::png(bytes)?,
        Sniffed::Jpeg => raster::jpeg(bytes)?,
        Sniffed::Gif => raster::gif(bytes)?,
        Sniffed::Webp => raster::webp(bytes)?,
        Sniffed::Wav => audio::wav(bytes)?,
        Sniffed::Mp3 => audio::mp3(bytes)?,
        Sniffed::Ogg => audio::ogg(bytes)?,
        Sniffed::Flac => audio::flac(bytes)?,
        Sniffed::Aac => audio::aac(bytes)?,
        Sniffed::Mp4 => container::isobmff(bytes, kind)?,
        Sniffed::Webm => container::webm(bytes, kind)?,
        Sniffed::QuickTime => container::quicktime(bytes)?,
    };
    let size = u64::try_from(clean.bytes.len()).unwrap_or(u64::MAX);
    kind.validate_size(size)?;
    Ok(clean)
}

fn kind_allows(kind: MediaKind, sniffed: Sniffed) -> bool {
    match kind {
        MediaKind::Photo | MediaKind::Avatar => {
            matches!(
                sniffed,
                Sniffed::Png | Sniffed::Jpeg | Sniffed::Gif | Sniffed::Webp
            )
        }
        MediaKind::Audio => matches!(
            sniffed,
            Sniffed::Wav
                | Sniffed::Mp3
                | Sniffed::Ogg
                | Sniffed::Flac
                | Sniffed::Aac
                | Sniffed::Mp4
                | Sniffed::Webm
        ),
        MediaKind::Video => matches!(sniffed, Sniffed::Mp4 | Sniffed::Webm | Sniffed::QuickTime),
    }
}

fn declared_conflicts(declared: &str, sniffed: Sniffed) -> bool {
    let mime = declared
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if mime.is_empty() || mime == "application/octet-stream" {
        return false;
    }
    !sniffed.allows_mime(&mime)
}

fn extension_conflicts(filename: &str, sniffed: Sniffed) -> bool {
    let Some(ext) = Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };
    let known = matches!(
        ext.as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "mp3"
            | "wav"
            | "ogg"
            | "m4a"
            | "aac"
            | "flac"
            | "mp4"
            | "webm"
            | "mov"
    );
    known && !sniffed.allows_extension(&ext)
}

impl Sniffed {
    fn allows_mime(self, mime: &str) -> bool {
        match self {
            Self::Png => mime == "image/png",
            Self::Jpeg => matches!(mime, "image/jpeg" | "image/jpg"),
            Self::Gif => mime == "image/gif",
            Self::Webp => mime == "image/webp",
            Self::Wav => matches!(mime, "audio/wav" | "audio/x-wav" | "audio/wave"),
            Self::Mp3 => matches!(mime, "audio/mpeg" | "audio/mp3"),
            Self::Ogg => mime == "audio/ogg",
            Self::Flac => matches!(mime, "audio/flac" | "audio/x-flac"),
            Self::Aac => mime == "audio/aac",
            Self::Mp4 => matches!(mime, "audio/mp4" | "video/mp4"),
            Self::Webm => matches!(mime, "audio/webm" | "video/webm"),
            Self::QuickTime => mime == "video/quicktime",
        }
    }

    fn allows_extension(self, ext: &str) -> bool {
        match self {
            Self::Png => ext == "png",
            Self::Jpeg => matches!(ext, "jpg" | "jpeg"),
            Self::Gif => ext == "gif",
            Self::Webp => ext == "webp",
            Self::Wav => ext == "wav",
            Self::Mp3 => ext == "mp3",
            Self::Ogg => ext == "ogg",
            Self::Flac => ext == "flac",
            Self::Aac => ext == "aac",
            Self::Mp4 => matches!(ext, "mp4" | "m4a"),
            Self::Webm => ext == "webm",
            Self::QuickTime => ext == "mov",
        }
    }
}

fn sniff(bytes: &[u8]) -> Option<Sniffed> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(Sniffed::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Sniffed::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(Sniffed::Gif);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") {
        return match &bytes[8..12] {
            b"WAVE" => Some(Sniffed::Wav),
            b"WEBP" => Some(Sniffed::Webp),
            _ => None,
        };
    }
    if bytes.starts_with(b"ID3") {
        return Some(Sniffed::Mp3);
    }
    if bytes.starts_with(b"OggS") {
        return Some(Sniffed::Ogg);
    }
    if bytes.starts_with(b"fLaC") {
        return Some(Sniffed::Flac);
    }
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return if bytes.len() >= 12 && &bytes[8..12] == b"qt  " {
            Some(Sniffed::QuickTime)
        } else {
            Some(Sniffed::Mp4)
        };
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(Sniffed::Webm);
    }
    if looks_like_mp3_frame(bytes) {
        return Some(Sniffed::Mp3);
    }
    if looks_like_adts(bytes) {
        return Some(Sniffed::Aac);
    }
    None
}

fn reject_hostile_prefix(bytes: &[u8]) -> Result<(), AppError> {
    const MAGICS: &[&[u8]] = &[
        b"MZ",
        b"\x7fELF",
        b"%PDF",
        b"PK\x03\x04",
        b"PK\x05\x06",
        b"\xca\xfe\xba\xbe",
        b"\xfe\xed\xfa\xce",
        b"\xfe\xed\xfa\xcf",
        b"\xcf\xfa\xed\xfe",
        b"#!",
    ];
    if MAGICS.iter().any(|magic| bytes.starts_with(magic)) {
        return Err(invalid("executable or archive contents"));
    }
    let head = &bytes[..bytes.len().min(256)];
    if contains_ignore_ascii_case(head, b"<html")
        || contains_ignore_ascii_case(head, b"<script")
        || contains_ignore_ascii_case(head, b"<!doctype")
        || contains_ignore_ascii_case(head, b"<svg")
        || contains_ignore_ascii_case(head, b"<?php")
        || contains_ignore_ascii_case(head, b"<?xml")
    {
        return Err(invalid("embedded markup"));
    }
    Ok(())
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn looks_like_mp3_frame(bytes: &[u8]) -> bool {
    audio::mp3_frame_len(bytes).is_some()
}

fn looks_like_adts(bytes: &[u8]) -> bool {
    bytes.len() >= 7 && bytes[0] == 0xFF && bytes[1] & 0xF6 == 0xF0
}

pub(super) fn invalid(message: &str) -> AppError {
    AppError::Validation(format!("rejected media: {message}"))
}

pub(super) fn validate_dims(width: u32, height: u32) -> Result<(), AppError> {
    if width == 0 || height == 0 {
        return Err(invalid("image has empty dimensions"));
    }
    if width > MAX_EDGE || height > MAX_EDGE {
        return Err(invalid("image dimensions are too large"));
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > MAX_PIXELS {
        return Err(invalid("image pixel count is too large"));
    }
    Ok(())
}

pub(super) fn as_i32_dim(value: u32) -> Option<i32> {
    i32::try_from(value).ok()
}

pub(super) fn need(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], AppError> {
    bytes
        .get(
            offset
                ..offset
                    .checked_add(len)
                    .ok_or_else(|| invalid("truncated"))?,
        )
        .ok_or_else(|| invalid("truncated"))
}

pub(super) fn u16_le(bytes: &[u8], offset: usize) -> Result<u16, AppError> {
    Ok(u16::from_le_bytes(
        need(bytes, offset, 2)?
            .try_into()
            .map_err(|_| invalid("truncated"))?,
    ))
}

pub(super) fn u16_be(bytes: &[u8], offset: usize) -> Result<u16, AppError> {
    Ok(u16::from_be_bytes(
        need(bytes, offset, 2)?
            .try_into()
            .map_err(|_| invalid("truncated"))?,
    ))
}

pub(super) fn u32_le(bytes: &[u8], offset: usize) -> Result<u32, AppError> {
    Ok(u32::from_le_bytes(
        need(bytes, offset, 4)?
            .try_into()
            .map_err(|_| invalid("truncated"))?,
    ))
}

pub(super) fn u32_be(bytes: &[u8], offset: usize) -> Result<u32, AppError> {
    Ok(u32::from_be_bytes(
        need(bytes, offset, 4)?
            .try_into()
            .map_err(|_| invalid("truncated"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{Sniffed, sanitize, sniff};
    use crate::modules::media::kinds::MediaKind;

    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    const WAV: &[u8] =
        b"RIFF$\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0D\xac\0\0\x88X\x01\0\x02\0\x10\0data\0\0\0\0";

    #[test]
    fn sniffs_png_and_wav() {
        assert_eq!(sniff(PNG), Some(Sniffed::Png));
        assert_eq!(sniff(WAV), Some(Sniffed::Wav));
        assert_eq!(sniff(b"<html>"), None);
    }

    #[test]
    fn png_is_rewritten_to_pixels_only() {
        let mut polyglot = PNG.to_vec();
        polyglot.extend_from_slice(b"<html><script>alert(1)</script>");
        let clean = sanitize(MediaKind::Photo, "shot.png", "image/png", &polyglot).unwrap();
        assert_eq!(clean.mime, "image/png");
        assert_eq!(clean.extension, "png");
        assert_eq!(clean.width, Some(1));
        assert_eq!(clean.height, Some(1));
        assert!(!clean.bytes.windows(7).any(|w| w == b"<script"));
        image::load_from_memory(&clean.bytes).unwrap();
    }

    #[test]
    fn markup_and_executables_are_rejected() {
        assert!(sanitize(MediaKind::Photo, "x.png", "image/png", b"<html><script>").is_err());
        assert!(sanitize(MediaKind::Photo, "x.png", "image/png", b"MZ\x90\x00").is_err());
        assert!(sanitize(MediaKind::Photo, "note.txt", "text/plain", b"hello").is_err());
        assert!(sanitize(MediaKind::Video, "clip.webm", "video/webm", PNG).is_err());
    }

    #[test]
    fn webp_is_rewritten_as_webp() {
        let mut raw = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut raw)
            .encode(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let clean = sanitize(MediaKind::Avatar, "face.webp", "image/webp", &raw).unwrap();
        assert_eq!(clean.mime, "image/webp");
        assert_eq!(clean.extension, "webp");
        assert_eq!(clean.width, Some(1));
        assert_eq!(clean.height, Some(1));
        assert!(clean.bytes.starts_with(b"RIFF"));
        assert_eq!(&clean.bytes[8..12], b"WEBP");
    }

    #[test]
    fn wav_roundtrip_keeps_a_valid_container() {
        let clean = sanitize(MediaKind::Audio, "tiny.wav", "audio/wav", WAV).unwrap();
        assert_eq!(clean.mime, "audio/wav");
        assert!(clean.bytes.starts_with(b"RIFF"));
    }

    #[test]
    fn tiny_webm_is_accepted_as_video() {
        let webm = include_bytes!("../../../../tests/fixtures/tiny.webm");
        let clean = sanitize(MediaKind::Video, "tiny.webm", "video/webm", webm).unwrap();
        assert_eq!(clean.mime, "video/webm");
        assert_eq!(clean.extension, "webm");
    }
}
