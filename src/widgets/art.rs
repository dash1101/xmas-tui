#![cfg_attr(windows, allow(dead_code))]
//! Pictures for shelf items where the system doesn't hand us a thumbnail
//! (Linux; Windows' shell makes its own): an image file's own picture, the
//! cover art inside a music file (MP3, FLAC, M4A/MP4, Ogg Vorbis / Opus),
//! or an album folder's cover image. Only the parts of a file that hold
//! the art are read (an MP3's tag, a FLAC's metadata, an M4A's `moov`), so
//! a shelf full of songs doesn't read whole files.

use crate::frame::Picture;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Thumbnails are kept at most this big (enough for a crisp "real" icon).
const MAX: usize = 96;
/// Don't decode anything bigger than this (bytes).
const LIMIT: u64 = 16 << 20;

/// A thumbnail for `path` (a folder: its cover image), if there's one.
pub fn thumb(path: &Path, dir: bool) -> Option<Picture> {
    if dir {
        return folder_cover(path);
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let bytes = match ext.as_str() {
        "png" | "jpg" | "jpeg" => return decode(&read_all(path)?),
        "mp3" => mp3_art(path)?,
        "flac" => flac_art(path)?,
        "m4a" | "mp4" | "m4b" | "aac" | "alac" => mp4_art(path)?,
        "ogg" | "opus" | "oga" => ogg_art(path)?,
        _ => return None,
    };
    // album art is square: a wide video thumbnail (as downloaders embed)
    // has the cover in its middle
    decode(&bytes).map(square)
}

/// The centre square of a picture.
fn square(p: Picture) -> Picture {
    if p.w == p.h {
        return p;
    }
    let n = p.w.min(p.h);
    let (x0, y0) = ((p.w - n) / 2, (p.h - n) / 2);
    let mut out = Vec::with_capacity(n * n * 4);
    for y in y0..y0 + n {
        out.extend_from_slice(&p.rgba[(y * p.w + x0) * 4..(y * p.w + x0 + n) * 4]);
    }
    Picture::new(n, n, out)
}

/// Music files fall back to the folder's cover (cover.jpg and friends).
pub fn thumb_or_folder(path: &Path, dir: bool) -> Option<Picture> {
    thumb(path, dir).or_else(|| {
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        let music = matches!(ext.as_str(), "mp3" | "flac" | "m4a" | "mp4" | "m4b" | "aac" | "alac" | "ogg" | "opus" | "oga" | "wav");
        if music {
            path.parent().and_then(folder_cover)
        } else {
            None
        }
    })
}

fn read_all(path: &Path) -> Option<Vec<u8>> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > LIMIT {
        return None;
    }
    std::fs::read(path).ok()
}

/// cover.jpg, folder.png, front.jpg… in a folder.
fn folder_cover(dir: &Path) -> Option<Picture> {
    let names = ["cover", "folder", "front", "album", "albumart", "albumartsmall"];
    let rd = std::fs::read_dir(dir).ok()?;
    let mut found: Vec<(usize, std::path::PathBuf)> = vec![];
    for e in rd.flatten() {
        let p = e.path();
        let (Some(stem), Some(ext)) = (p.file_stem(), p.extension()) else { continue };
        let (stem, ext) = (stem.to_string_lossy().to_lowercase(), ext.to_string_lossy().to_lowercase());
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
            continue;
        }
        if let Some(k) = names.iter().position(|n| stem == *n || (*n == "albumart" && stem.starts_with("albumart_"))) {
            found.push((k, p));
        }
    }
    found.sort();
    found.into_iter().find_map(|(_, p)| decode(&read_all(&p)?).map(square))
}

/// PNG or JPEG bytes → a thumbnail (straight RGBA).
pub fn decode(bytes: &[u8]) -> Option<Picture> {
    if bytes.starts_with(b"\x89PNG") {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = dec.read_info().ok()?;
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).ok()?;
        let (w, h) = (info.width as usize, info.height as usize);
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
            png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&c| [c, c, c, 255]).collect(),
            _ => return None,
        };
        return Some(shrink(w, h, &rgba));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
        dec.read_info().ok()?;
        // decode at a fraction of the size (the JPEG's own downscaling)
        dec.scale(MAX as u16, MAX as u16).ok()?;
        let px = dec.decode().ok()?;
        let info = dec.info()?;
        let (w, h) = (info.width as usize, info.height as usize);
        use jpeg_decoder::PixelFormat as F;
        let rgba: Vec<u8> = match info.pixel_format {
            F::RGB24 => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            F::L8 => px.iter().flat_map(|&c| [c, c, c, 255]).collect(),
            F::L16 => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], 255]).collect(),
            F::CMYK32 => px
                .chunks(4)
                .flat_map(|c| {
                    let k = 255 - c[3] as u32;
                    let f = |v: u8| ((255 - v as u32) * k / 255) as u8;
                    [f(c[0]), f(c[1]), f(c[2]), 255]
                })
                .collect(),
        };
        if rgba.len() < w * h * 4 {
            return None;
        }
        return Some(shrink(w, h, &rgba));
    }
    None
}

/// Box-filter down to at most MAX on the longer side.
fn shrink(w: usize, h: usize, rgba: &[u8]) -> Picture {
    if w <= MAX && h <= MAX {
        return Picture::new(w, h, rgba[..w * h * 4].to_vec());
    }
    let k = (w.max(h) as f32 / MAX as f32).max(1.0);
    let (nw, nh) = (((w as f32 / k).round() as usize).max(1), ((h as f32 / k).round() as usize).max(1));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut acc = [0u32; 4];
            let mut n = 0;
            for yy in y0..y1.min(h) {
                for xx in x0..x1.min(w) {
                    let i = (yy * w + xx) * 4;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            for c in 0..4 {
                out[(y * nw + x) * 4 + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    Picture::new(nw, nh, out)
}

fn read_at(f: &mut std::fs::File, at: u64, n: usize) -> Option<Vec<u8>> {
    if n as u64 > LIMIT {
        return None;
    }
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut b = vec![0; n];
    f.read_exact(&mut b).ok()?;
    Some(b)
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// The picture in an ID3v2 tag (APIC, or PIC in v2.2): the front cover if
/// there is one, else the first.
fn mp3_art(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let head = read_at(&mut f, 0, 10)?;
    if &head[..3] != b"ID3" {
        return None;
    }
    let ver = head[3];
    let syncsafe = |b: &[u8]| (b[0] as u32 & 0x7f) << 21 | (b[1] as u32 & 0x7f) << 14 | (b[2] as u32 & 0x7f) << 7 | (b[3] as u32 & 0x7f);
    let size = syncsafe(&head[6..10]) as usize;
    let tag = read_at(&mut f, 10, size)?;
    let mut off = 0;
    // an extended header (v2.3 / v2.4) comes first when flagged
    if head[5] & 0x40 != 0 && tag.len() >= 4 {
        off = if ver == 4 { syncsafe(&tag[..4]) as usize } else { be32(&tag[..4]) as usize + 4 };
    }
    let mut best: Option<Vec<u8>> = None;
    while off + 10 <= tag.len() {
        let (id, len, body) = if ver == 2 {
            let len = (tag[off + 3] as usize) << 16 | (tag[off + 4] as usize) << 8 | tag[off + 5] as usize;
            (&tag[off..off + 3], len, off + 6)
        } else {
            let len = if ver == 4 { syncsafe(&tag[off + 4..off + 8]) } else { be32(&tag[off + 4..off + 8]) } as usize;
            (&tag[off..off + 4], len, off + 10)
        };
        if id[0] == 0 || len == 0 || body + len > tag.len() {
            break;
        }
        let d = &tag[body..body + len];
        if id == b"APIC" || id == b"PIC" {
            if let Some((kind, img)) = apic(d, ver == 2) {
                if kind == 3 || best.is_none() {
                    best = Some(img.to_vec());
                    if kind == 3 {
                        break;
                    }
                }
            }
        }
        off = body + len;
    }
    best
}

/// An APIC / PIC frame's picture type and image bytes.
fn apic(d: &[u8], v22: bool) -> Option<(u8, &[u8])> {
    let enc = *d.first()?;
    let mut i = 1;
    if v22 {
        i += 3; // "JPG" / "PNG"
    } else {
        i += d[i..].iter().position(|&b| b == 0)? + 1; // MIME type
    }
    let kind = *d.get(i)?;
    i += 1;
    // the description, ending in a NUL (two for UTF-16)
    if enc == 1 || enc == 2 {
        while i + 1 < d.len() && !(d[i] == 0 && d[i + 1] == 0) {
            i += 2;
        }
        i += 2;
    } else {
        i += d[i..].iter().position(|&b| b == 0)? + 1;
    }
    d.get(i..).filter(|b| !b.is_empty()).map(|b| (kind, b))
}

/// A FLAC's PICTURE metadata block.
fn flac_art(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    if read_at(&mut f, 0, 4)? != b"fLaC" {
        return None;
    }
    let mut at = 4u64;
    for _ in 0..64 {
        let h = read_at(&mut f, at, 4)?;
        let (last, kind, len) = (h[0] & 0x80 != 0, h[0] & 0x7f, (h[1] as usize) << 16 | (h[2] as usize) << 8 | h[3] as usize);
        if kind == 6 {
            return picture_block(&read_at(&mut f, at + 4, len)?);
        }
        if last {
            break;
        }
        at += 4 + len as u64;
    }
    None
}

/// A FLAC picture block (also inside Ogg comments): its image bytes.
fn picture_block(b: &[u8]) -> Option<Vec<u8>> {
    let mut i = 4;
    let mime = be32(b.get(i..i + 4)?) as usize;
    i += 4 + mime;
    let desc = be32(b.get(i..i + 4)?) as usize;
    i += 4 + desc + 16;
    let len = be32(b.get(i..i + 4)?) as usize;
    i += 4;
    b.get(i..i + len).map(|x| x.to_vec())
}

/// An M4A's cover: moov/udta/meta/ilst/covr/data (the moov box may be
/// anywhere in the file; the audio in mdat is skipped, not read).
fn mp4_art(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut at = 0u64;
    while at + 8 <= len {
        let h = read_at(&mut f, at, 8)?;
        let (mut size, kind) = (be32(&h[..4]) as u64, &h[4..8]);
        let mut hdr = 8;
        if size == 1 {
            size = u64::from_be_bytes(read_at(&mut f, at + 8, 8)?.try_into().ok()?);
            hdr = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < hdr {
            return None;
        }
        if kind == b"moov" {
            let moov = read_at(&mut f, at + hdr, (size - hdr) as usize)?;
            return find_covr(&moov);
        }
        at += size;
    }
    None
}

/// Walk boxes down to covr/data.
fn find_covr(b: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i + 8 <= b.len() {
        let size = be32(&b[i..i + 4]) as usize;
        let kind = &b[i + 4..i + 8];
        if size < 8 || i + size > b.len() {
            return None;
        }
        let body = &b[i + 8..i + size];
        match kind {
            b"udta" | b"ilst" | b"trak" => {
                if let Some(x) = find_covr(body) {
                    return Some(x);
                }
            }
            // meta has 4 bytes of version and flags first
            b"meta" if body.len() > 4 => {
                if let Some(x) = find_covr(&body[4..]) {
                    return Some(x);
                }
            }
            b"covr" => {
                // covr/data: 8 bytes of type and locale, then the image
                let mut j = 0;
                while j + 16 <= body.len() {
                    let ds = be32(&body[j..j + 4]) as usize;
                    if &body[j + 4..j + 8] == b"data" && ds >= 16 && j + ds <= body.len() {
                        return Some(body[j + 16..j + ds].to_vec());
                    }
                    if ds < 8 {
                        break;
                    }
                    j += ds;
                }
            }
            _ => {}
        }
        i += size;
    }
    None
}

/// Ogg Vorbis / Opus: a METADATA_BLOCK_PICTURE comment (a base64 FLAC
/// picture block). The comment packet can span pages, so the pages'
/// segments are joined first.
fn ogg_art(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut raw = vec![];
    f.by_ref().take(4 << 20).read_to_end(&mut raw).ok()?;
    let mut data = vec![];
    let mut i = 0;
    while i + 27 <= raw.len() && &raw[i..i + 4] == b"OggS" {
        let segs = raw[i + 26] as usize;
        let table = raw.get(i + 27..i + 27 + segs)?;
        let body: usize = table.iter().map(|&s| s as usize).sum();
        let start = i + 27 + segs;
        data.extend_from_slice(raw.get(start..start + body)?);
        i = start + body;
        // the art is in the second packet; stop once audio pages begin
        if data.len() > 3 << 20 {
            break;
        }
    }
    let key = b"METADATA_BLOCK_PICTURE=";
    let at = data.windows(key.len()).position(|w| w.eq_ignore_ascii_case(key))? + key.len();
    let b64: Vec<u8> = data[at..].iter().copied().take_while(|c| c.is_ascii_alphanumeric() || *c == b'+' || *c == b'/' || *c == b'=').collect();
    picture_block(&unbase64(&b64)?)
}

fn unbase64(s: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for &c in s {
        if c == b'=' {
            break;
        }
        acc = acc << 6 | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny PNG made on the spot, wrapped the way each format keeps art.
    fn png_bytes() -> Vec<u8> {
        let mut out = vec![];
        {
            let mut e = png::Encoder::new(&mut out, 2, 2);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]).unwrap();
        }
        out
    }

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("xmas-art-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn finds_art_in_an_m4a() {
        let img = png_bytes();
        fn bx(kind: &[u8], body: &[u8]) -> Vec<u8> {
            let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            v.extend_from_slice(kind);
            v.extend_from_slice(body);
            v
        }
        let mut data = vec![0, 0, 0, 14, 0, 0, 0, 0];
        data.extend_from_slice(&img);
        let covr = bx(b"covr", &bx(b"data", &data));
        let meta = bx(b"meta", &[&[0u8, 0, 0, 0][..], &bx(b"ilst", &covr)].concat());
        let moov = bx(b"moov", &bx(b"udta", &meta));
        let file = [bx(b"ftyp", b"M4A \0\0\0\0"), bx(b"mdat", &[0u8; 64]), moov].concat();
        let p = dir().join("song.m4a");
        std::fs::write(&p, file).unwrap();
        let pic = thumb(&p, false).expect("no art found");
        assert_eq!((pic.w, pic.h), (2, 2));
        assert_eq!(&pic.rgba[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn finds_art_in_an_mp3_and_a_flac() {
        let img = png_bytes();
        // ID3v2.3 with an APIC frame (front cover)
        let mut apic = vec![0u8];
        apic.extend_from_slice(b"image/png\0");
        apic.push(3);
        apic.extend_from_slice(b"cover\0");
        apic.extend_from_slice(&img);
        let mut frame = b"APIC".to_vec();
        frame.extend_from_slice(&(apic.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&apic);
        let n = frame.len() as u32;
        let mut mp3 = b"ID3\x03\x00\x00".to_vec();
        mp3.extend_from_slice(&[(n >> 21 & 0x7f) as u8, (n >> 14 & 0x7f) as u8, (n >> 7 & 0x7f) as u8, (n & 0x7f) as u8]);
        mp3.extend_from_slice(&frame);
        mp3.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        let p = dir().join("song.mp3");
        std::fs::write(&p, mp3).unwrap();
        assert!(thumb(&p, false).is_some(), "no art in the mp3");
        // FLAC: a PICTURE block, last
        let mut pb = vec![];
        pb.extend_from_slice(&3u32.to_be_bytes());
        pb.extend_from_slice(&9u32.to_be_bytes());
        pb.extend_from_slice(b"image/png");
        pb.extend_from_slice(&0u32.to_be_bytes());
        pb.extend_from_slice(&[0u8; 16]);
        pb.extend_from_slice(&(img.len() as u32).to_be_bytes());
        pb.extend_from_slice(&img);
        let mut flac = b"fLaC".to_vec();
        flac.push(0x80 | 6);
        flac.extend_from_slice(&(pb.len() as u32).to_be_bytes()[1..]);
        flac.extend_from_slice(&pb);
        let p = dir().join("song.flac");
        std::fs::write(&p, flac).unwrap();
        assert!(thumb(&p, false).is_some(), "no art in the flac");
    }

    #[test]
    fn an_album_folder_shows_its_cover() {
        let d = dir().join("Album");
        let _ = std::fs::create_dir_all(&d);
        std::fs::write(d.join("Cover.png"), png_bytes()).unwrap();
        std::fs::write(d.join("track.ogg"), b"not really ogg").unwrap();
        assert!(thumb(&d, true).is_some(), "no folder cover");
        assert!(thumb_or_folder(&d.join("track.ogg"), false).is_some(), "a track without art doesn't use the folder's");
    }
}
