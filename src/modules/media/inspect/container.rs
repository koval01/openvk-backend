use bytes::Bytes;

use crate::error::AppError;
use crate::modules::media::inspect::{CleanMedia, invalid, need, u32_be};
use crate::modules::media::kinds::MediaKind;

const MAX_BOXES: usize = 20_000;
const MAX_DEPTH: u8 = 12;

pub fn isobmff(bytes: &[u8], kind: MediaKind) -> Result<CleanMedia, AppError> {
    walk_isobmff(bytes, 0, true)?;
    if kind == MediaKind::Audio {
        Ok(CleanMedia {
            bytes: Bytes::copy_from_slice(bytes),
            mime: "audio/mp4".into(),
            extension: "m4a",
            width: None,
            height: None,
        })
    } else {
        Ok(CleanMedia {
            bytes: Bytes::copy_from_slice(bytes),
            mime: "video/mp4".into(),
            extension: "mp4",
            width: None,
            height: None,
        })
    }
}

pub fn quicktime(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    walk_isobmff(bytes, 0, true)?;
    Ok(CleanMedia {
        bytes: Bytes::copy_from_slice(bytes),
        mime: "video/quicktime".into(),
        extension: "mov",
        width: None,
        height: None,
    })
}

pub fn webm(bytes: &[u8], kind: MediaKind) -> Result<CleanMedia, AppError> {
    let mut offset = 0;
    let mut doctype = None;
    walk_ebml(bytes, &mut offset, 0, &mut doctype, 0)?;
    if offset != bytes.len() {
        return Err(invalid("trailing data after webm"));
    }
    if doctype.as_deref() != Some("webm") {
        return Err(invalid("webm document type"));
    }
    if kind == MediaKind::Audio {
        Ok(CleanMedia {
            bytes: Bytes::copy_from_slice(bytes),
            mime: "audio/webm".into(),
            extension: "webm",
            width: None,
            height: None,
        })
    } else {
        Ok(CleanMedia {
            bytes: Bytes::copy_from_slice(bytes),
            mime: "video/webm".into(),
            extension: "webm",
            width: None,
            height: None,
        })
    }
}

fn walk_isobmff(bytes: &[u8], depth: u8, require_ftyp: bool) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        return Err(invalid("mp4 structure"));
    }
    let mut i = 0;
    let mut boxes = 0usize;
    let mut saw_ftyp = false;
    while i < bytes.len() {
        if bytes.len() - i < 8 {
            return Err(invalid("mp4 box"));
        }
        let size32 = u32_be(bytes, i)?;
        let typ = need(bytes, i + 4, 4)?;
        if matches!(typ, b"xml " | b"XML " | b"uuid") {
            return Err(invalid("mp4 metadata box"));
        }
        let (header, payload) = match size32 {
            0 => {
                let rest = bytes.len() - i;
                if rest < 8 {
                    return Err(invalid("mp4 box"));
                }
                (8usize, rest - 8)
            }
            1 => {
                if bytes.len() - i < 16 {
                    return Err(invalid("mp4 box"));
                }
                let size64 = u64::from_be_bytes(
                    need(bytes, i + 8, 8)?
                        .try_into()
                        .map_err(|_| invalid("mp4 box"))?,
                );
                let total = usize::try_from(size64).map_err(|_| invalid("mp4 box"))?;
                if total < 16 {
                    return Err(invalid("mp4 box"));
                }
                (16usize, total - 16)
            }
            n => {
                let total = n as usize;
                if total < 8 {
                    return Err(invalid("mp4 box"));
                }
                (8usize, total - 8)
            }
        };
        let payload_start = i + header;
        let payload_end = payload_start
            .checked_add(payload)
            .ok_or_else(|| invalid("mp4 box"))?;
        if payload_end > bytes.len() {
            return Err(invalid("mp4 box"));
        }
        if require_ftyp && depth == 0 && i == 0 && typ != b"ftyp" {
            return Err(invalid("mp4 brand"));
        }
        if typ == b"ftyp" {
            saw_ftyp = true;
            if payload < 4 {
                return Err(invalid("mp4 brand"));
            }
        }
        if is_iso_container(typ) {
            walk_isobmff(&bytes[payload_start..payload_end], depth + 1, false)?;
        }
        i = payload_end;
        boxes += 1;
        if boxes > MAX_BOXES {
            return Err(invalid("mp4 structure"));
        }
    }
    if require_ftyp && !saw_ftyp {
        return Err(invalid("mp4 brand"));
    }
    Ok(())
}

fn is_iso_container(typ: &[u8]) -> bool {
    matches!(
        typ,
        b"moov"
            | b"trak"
            | b"mdia"
            | b"minf"
            | b"stbl"
            | b"dinf"
            | b"edts"
            | b"udta"
            | b"moof"
            | b"traf"
            | b"mvex"
            | b"meta"
    )
}

fn walk_ebml(
    bytes: &[u8],
    offset: &mut usize,
    end: usize,
    doctype: &mut Option<String>,
    depth: u8,
) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        return Err(invalid("webm structure"));
    }
    let limit = if end == 0 { bytes.len() } else { end };
    let mut seen = 0usize;
    while *offset < limit {
        let id = read_ebml_id(bytes, offset)?;
        let size = read_vint_size(bytes, offset)?;
        if size == u64::MAX {
            return Err(invalid("webm element"));
        }
        let payload = usize::try_from(size).map_err(|_| invalid("webm element"))?;
        let payload_end = offset
            .checked_add(payload)
            .ok_or_else(|| invalid("webm element"))?;
        if payload_end > limit {
            return Err(invalid("webm element"));
        }
        if id == 0x4282 {
            let text = std::str::from_utf8(&bytes[*offset..payload_end])
                .map_err(|_| invalid("webm document type"))?
                .trim()
                .to_ascii_lowercase();
            *doctype = Some(text);
        }
        if is_ebml_master(id) {
            let nested_end = payload_end;
            walk_ebml(bytes, offset, nested_end, doctype, depth + 1)?;
            if *offset != nested_end {
                return Err(invalid("webm element"));
            }
        } else {
            *offset = payload_end;
        }
        seen += 1;
        if seen > MAX_BOXES {
            return Err(invalid("webm structure"));
        }
    }
    Ok(())
}

fn is_ebml_master(id: u64) -> bool {
    id == 0x1A45_DFA3
}

fn vint_width(bytes: &[u8], offset: usize) -> Result<usize, AppError> {
    let first = *bytes.get(offset).ok_or_else(|| invalid("webm element"))?;
    if first == 0 {
        return Err(invalid("webm element"));
    }
    let width = usize::try_from(first.leading_zeros())
        .unwrap_or(8)
        .saturating_add(1);
    if (1..=8).contains(&width) && offset + width <= bytes.len() {
        Ok(width)
    } else {
        Err(invalid("webm element"))
    }
}

fn read_ebml_id(bytes: &[u8], offset: &mut usize) -> Result<u64, AppError> {
    let width = vint_width(bytes, *offset)?;
    let mut value = 0_u64;
    for byte in &bytes[*offset..*offset + width] {
        value = (value << 8) | u64::from(*byte);
    }
    *offset += width;
    Ok(value)
}

fn read_vint_size(bytes: &[u8], offset: &mut usize) -> Result<u64, AppError> {
    let width = vint_width(bytes, *offset)?;
    let first = bytes[*offset];
    let mut value = u64::from(first) & (0xFF >> width);
    let unknown = (1_u64 << (7 * width)) - 1;
    for byte in &bytes[*offset + 1..*offset + width] {
        value = (value << 8) | u64::from(*byte);
    }
    *offset += width;
    if value == unknown {
        return Err(invalid("webm element"));
    }
    Ok(value)
}
