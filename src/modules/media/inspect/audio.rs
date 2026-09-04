use bytes::Bytes;

use crate::error::AppError;
use crate::modules::media::inspect::{CleanMedia, invalid, need, u16_le, u32_le};

pub fn wav(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    if bytes.len() < 12 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WAVE" {
        return Err(invalid("wav header"));
    }
    let declared = u32_le(bytes, 4)? as usize;
    let end = declared
        .checked_add(8)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| invalid("wav size"))?;
    let mut i = 12;
    let mut fmt = None;
    let mut data = None;
    while i + 8 <= end {
        let fourcc = need(bytes, i, 4)?;
        let size = u32_le(bytes, i + 4)? as usize;
        let data_start = i + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| invalid("wav chunk"))?;
        if data_end > end {
            return Err(invalid("wav chunk"));
        }
        match fourcc {
            b"fmt " => {
                if size < 16 {
                    return Err(invalid("wav format"));
                }
                let format = u16_le(bytes, data_start)?;
                if !matches!(format, 1 | 3 | 0xFFFE) {
                    return Err(invalid("wav format"));
                }
                fmt = Some(&bytes[i..data_end]);
            }
            b"data" => data = Some(&bytes[i..data_end]),
            b"LIST" | b"JUNK" | b"fact" | b"PEAK" => {}
            _ => return Err(invalid("wav chunk")),
        }
        i = data_end + usize::from(size % 2 == 1);
        i = i.min(end);
    }
    let fmt = fmt.ok_or_else(|| invalid("wav format"))?;
    let data = data.ok_or_else(|| invalid("wav data"))?;
    let mut out = Vec::with_capacity(12 + fmt.len() + data.len());
    let payload = fmt.len() + data.len() + 4;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(
        &u32::try_from(payload)
            .map_err(|_| invalid("wav size"))?
            .to_le_bytes(),
    );
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(fmt);
    if fmt.len() % 2 == 1 {
        out.push(0);
    }
    out.extend_from_slice(data);
    Ok(CleanMedia {
        bytes: Bytes::from(out),
        mime: "audio/wav".into(),
        extension: "wav",
        width: None,
        height: None,
    })
}

pub fn mp3(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let mut i = 0;
    if bytes.starts_with(b"ID3") && bytes.len() >= 10 {
        let size = id3v2_size(&bytes[6..10])?;
        i = 10usize
            .checked_add(size)
            .ok_or_else(|| invalid("mp3 tag"))?;
        if i > bytes.len() {
            return Err(invalid("mp3 tag"));
        }
    }
    let start = i;
    let mut frames = 0usize;
    while i + 4 <= bytes.len() {
        if bytes[i..].starts_with(b"TAG") && bytes.len() - i >= 128 {
            break;
        }
        let Some(len) = mp3_frame_len(&bytes[i..]) else {
            return Err(invalid("mp3 frame"));
        };
        i = i.checked_add(len).ok_or_else(|| invalid("mp3 frame"))?;
        if i > bytes.len() {
            return Err(invalid("mp3 frame"));
        }
        frames += 1;
        if frames > 500_000 {
            return Err(invalid("mp3 frame"));
        }
    }
    if frames == 0 {
        return Err(invalid("mp3 frame"));
    }
    if i < bytes.len() && !bytes[i..].starts_with(b"TAG") {
        return Err(invalid("trailing data after mp3"));
    }
    if i < bytes.len() && bytes.len() - i != 128 {
        return Err(invalid("trailing data after mp3"));
    }
    Ok(CleanMedia {
        bytes: Bytes::copy_from_slice(&bytes[start..i]),
        mime: "audio/mpeg".into(),
        extension: "mp3",
        width: None,
        height: None,
    })
}

pub fn ogg(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let mut i = 0;
    let mut pages = 0usize;
    while i < bytes.len() {
        if i + 27 > bytes.len() || !bytes[i..].starts_with(b"OggS") {
            return Err(invalid("ogg page"));
        }
        if bytes[i + 4] != 0 {
            return Err(invalid("ogg page"));
        }
        let nsegs = usize::from(bytes[i + 26]);
        let table = i + 27;
        if table + nsegs > bytes.len() {
            return Err(invalid("ogg page"));
        }
        let body: usize = bytes[table..table + nsegs]
            .iter()
            .map(|seg| usize::from(*seg))
            .sum();
        i = table
            .checked_add(nsegs)
            .and_then(|offset| offset.checked_add(body))
            .ok_or_else(|| invalid("ogg page"))?;
        if i > bytes.len() {
            return Err(invalid("ogg page"));
        }
        pages += 1;
        if pages > 500_000 {
            return Err(invalid("ogg page"));
        }
    }
    if pages == 0 {
        return Err(invalid("ogg page"));
    }
    Ok(CleanMedia {
        bytes: Bytes::copy_from_slice(bytes),
        mime: "audio/ogg".into(),
        extension: "ogg",
        width: None,
        height: None,
    })
}

pub fn flac(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    if !bytes.starts_with(b"fLaC") || bytes.len() < 42 {
        return Err(invalid("flac header"));
    }
    let mut i = 4;
    let mut last = false;
    let mut streaminfo = None;
    while !last {
        if i + 4 > bytes.len() {
            return Err(invalid("flac metadata"));
        }
        let header = bytes[i];
        last = header & 0x80 != 0;
        let block_type = header & 0x7F;
        let size = (u32::from(bytes[i + 1]) << 16)
            | (u32::from(bytes[i + 2]) << 8)
            | u32::from(bytes[i + 3]);
        let size = size as usize;
        let data_end = i
            .checked_add(4)
            .and_then(|offset| offset.checked_add(size))
            .ok_or_else(|| invalid("flac metadata"))?;
        if data_end > bytes.len() {
            return Err(invalid("flac metadata"));
        }
        match block_type {
            0 => streaminfo = Some(&bytes[i..data_end]),
            1..=6 => {}
            _ => return Err(invalid("flac metadata")),
        }
        i = data_end;
    }
    let streaminfo = streaminfo.ok_or_else(|| invalid("flac stream"))?;
    if i >= bytes.len() {
        return Err(invalid("flac stream"));
    }
    let mut keep = Vec::from(&bytes[..4]);
    keep.push(streaminfo[0] | 0x80);
    keep.extend_from_slice(&streaminfo[1..]);
    keep.extend_from_slice(&bytes[i..]);
    Ok(CleanMedia {
        bytes: Bytes::from(keep),
        mime: "audio/flac".into(),
        extension: "flac",
        width: None,
        height: None,
    })
}

pub fn aac(bytes: &[u8]) -> Result<CleanMedia, AppError> {
    let mut i = 0;
    let mut frames = 0usize;
    while i + 7 <= bytes.len() {
        if bytes[i] != 0xFF || bytes[i + 1] & 0xF6 != 0xF0 {
            return Err(invalid("aac frame"));
        }
        let len = ((usize::from(bytes[i + 3]) & 0x03) << 11)
            | (usize::from(bytes[i + 4]) << 3)
            | (usize::from(bytes[i + 5]) >> 5);
        if len < 7 {
            return Err(invalid("aac frame"));
        }
        i = i.checked_add(len).ok_or_else(|| invalid("aac frame"))?;
        if i > bytes.len() {
            return Err(invalid("aac frame"));
        }
        frames += 1;
        if frames > 500_000 {
            return Err(invalid("aac frame"));
        }
    }
    if frames == 0 || i != bytes.len() {
        return Err(invalid("aac frame"));
    }
    Ok(CleanMedia {
        bytes: Bytes::copy_from_slice(bytes),
        mime: "audio/aac".into(),
        extension: "aac",
        width: None,
        height: None,
    })
}

pub fn mp3_frame_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] & 0xE0 != 0xE0 {
        return None;
    }
    let header = u32::from_be_bytes(bytes[..4].try_into().ok()?);
    let version_id = (header >> 19) & 0b11;
    let layer = (header >> 17) & 0b11;
    let bitrate_idx = ((header >> 12) & 0xF) as usize;
    let sample_idx = ((header >> 10) & 0b11) as usize;
    let padding = (header >> 9) & 1;
    if version_id == 1 || layer == 0 || bitrate_idx == 0 || bitrate_idx == 15 || sample_idx == 3 {
        return None;
    }
    let bitrate = u64::from(bitrate_kbps(version_id, layer, bitrate_idx)?) * 1000;
    let sample_rate = u64::from(sample_rate_hz(version_id, sample_idx)?);
    let samples = match (version_id, layer) {
        (_, 3) => 384,
        (3, 2 | 1) => 1152,
        (_, 2 | 1) => 576,
        _ => return None,
    };
    let len = (samples * bitrate) / (8 * sample_rate) + u64::from(padding);
    usize::try_from(len).ok().filter(|len| *len >= 4)
}

fn id3v2_size(bytes: &[u8]) -> Result<usize, AppError> {
    if bytes.len() < 4 || bytes.iter().any(|byte| byte & 0x80 != 0) {
        return Err(invalid("mp3 tag"));
    }
    Ok(((usize::from(bytes[0]) & 0x7F) << 21)
        | ((usize::from(bytes[1]) & 0x7F) << 14)
        | ((usize::from(bytes[2]) & 0x7F) << 7)
        | (usize::from(bytes[3]) & 0x7F))
}

fn bitrate_kbps(version_id: u32, layer: u32, index: usize) -> Option<u32> {
    const V1_L1: [u32; 16] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0,
    ];
    const V1_L2: [u32; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0,
    ];
    const V1_L3: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const V2_L1: [u32; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0,
    ];
    const V2_L2: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    let table = match (version_id, layer) {
        (3, 3) => &V1_L1,
        (3, 2) => &V1_L2,
        (3, 1) => &V1_L3,
        (_, 3) => &V2_L1,
        (_, 2 | 1) => &V2_L2,
        _ => return None,
    };
    Some(table[index]).filter(|rate| *rate != 0)
}

fn sample_rate_hz(version_id: u32, index: usize) -> Option<u32> {
    let base = [44100, 48000, 32000].get(index).copied()?;
    Some(match version_id {
        3 => base,
        2 => base / 2,
        0 => base / 4,
        _ => return None,
    })
}
