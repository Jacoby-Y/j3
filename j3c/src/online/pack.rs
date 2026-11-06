//! Downloaded packs: zip unpacking, Radiance .hdr to PNG, and cached preview thumbnails.

use super::*;

/// Unpack `zip` into `dir`; returns how many files were written.
pub(crate) fn unzip(zip: &[u8], dir: &Path) -> Result<usize, String> {
    let bad = || "not a zip file (zip64 isn't supported)".to_string();
    let u16at = |i: usize| zip.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(bad);
    let u32at = |i: usize| zip.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(bad);
    let eocd = (0..zip.len().saturating_sub(21)).rev().take(65536 + 22).find(|&i| zip[i..i + 4] == [0x50, 0x4b, 5, 6]).ok_or_else(bad)?;
    let count = u16at(eocd + 10)?;
    let mut at = u32at(eocd + 16)?;
    if count == 0xffff || at == 0xffff_ffff { return Err(bad()); }
    let mut written = 0;
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 1, 2]) { return Err(bad()); }
        let (method, csize, size) = (u16at(at + 10)?, u32at(at + 20)?, u32at(at + 24)?);
        let (nlen, elen, clen, local) = (u16at(at + 28)?, u16at(at + 30)?, u16at(at + 32)?, u32at(at + 42)?);
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + nlen).ok_or_else(bad)?).into_owned();
        at += 46 + nlen + elen + clen;
        if name.ends_with('/') { continue; }
        safe_rel(&name)?;
        let start = local + 30 + u16at(local + 26)? + u16at(local + 28)?;
        let data = zip.get(start..start + csize).ok_or_else(bad)?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => crate::png::inflate(data, size).filter(|b| b.len() == size).ok_or(format!("{name}: bad compressed data"))?,
            m => return Err(format!("{name}: zip method {m} isn't supported")),
        };
        let dest = dir.join(&name);
        if let Some(parent) = dest.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
        written += 1;
    }
    Ok(written)
}

/// Radiance `.hdr` (flat or new-style RLE, `-Y h +X w`) tone-mapped (Reinhard, gamma 2.2) to
/// an RGBA8 PNG.
pub(crate) fn hdr_to_png(b: &[u8]) -> Result<Vec<u8>, String> {
    let (w, h, px) = hdr_decode(b)?;
    Ok(crate::png::encode_rgba(w, h, &px))
}

pub(super) fn hdr_decode(b: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let bad = |m: &str| format!(".hdr: {m}");
    if !b.starts_with(b"#?") { return Err(bad("not a Radiance file")); }
    let mut at = 0;
    let mut line = || -> Result<&[u8], String> {
        let end = at + b[at..].iter().position(|&c| c == b'\n').ok_or_else(|| bad("header never ends"))?;
        let l = &b[at..end];
        at = end + 1;
        Ok(l)
    };
    while !line()?.is_empty() {}
    let res = String::from_utf8_lossy(line()?).into_owned();
    let parts: Vec<&str> = res.split_whitespace().collect();
    let (w, h) = match parts[..] {
        ["-Y", h, "+X", w] => (w.parse::<usize>().map_err(|_| bad("bad width"))?, h.parse::<usize>().map_err(|_| bad("bad height"))?),
        _ => return Err(bad("only `-Y H +X W` images are supported")),
    };
    if w == 0 || h == 0 || w * h > 16384 * 8192 { return Err(bad("bad size")); }
    let mut rgbe = vec![0u8; w * h * 4];
    let short = || bad("data ends early");
    for y in 0..h {
        let row = &mut rgbe[y * w * 4..(y + 1) * w * 4];
        let rle = (8..32768).contains(&w) && b.get(at..at + 2) == Some(&[2, 2]) && b.get(at + 2).is_some_and(|&c| c & 0x80 == 0);
        if !rle {
            row.copy_from_slice(b.get(at..at + w * 4).ok_or_else(short)?);
            at += w * 4;
            continue;
        }
        if ((b[at + 2] as usize) << 8 | *b.get(at + 3).ok_or_else(short)? as usize) != w { return Err(bad("scanline width mismatch")); }
        at += 4;
        for c in 0..4 {
            let mut x = 0;
            while x < w {
                let n = *b.get(at).ok_or_else(short)? as usize;
                at += 1;
                if n > 128 {
                    let v = *b.get(at).ok_or_else(short)?;
                    at += 1;
                    if x + n - 128 > w { return Err(bad("run past the scanline")); }
                    for _ in 0..n - 128 { row[x * 4 + c] = v; x += 1; }
                } else {
                    if n == 0 || x + n > w { return Err(bad("bad run")); }
                    for &v in b.get(at..at + n).ok_or_else(short)? { row[x * 4 + c] = v; x += 1; }
                    at += n;
                }
            }
        }
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for p in rgbe.chunks(4) {
        let scale = if p[3] == 0 { 0.0 } else { 2f32.powi(p[3] as i32 - 136) };
        for &m in &p[..3] {
            let v = m as f32 * scale;
            out.push(((v / (1.0 + v)).powf(1.0 / 2.2) * 255.0).round() as u8);
        }
        out.push(255);
    }
    Ok((w, h, out))
}

/// The hit's thumbnail PNG in the cache, downloaded if needed.
pub(crate) fn thumb(hit: &Hit) -> Option<PathBuf> {
    if hit.thumb.is_empty() { return None; }
    let dest = cache(hit.source).ok()?.join("thumbs").join(format!("{}.png", hit.id));
    download(&hit.thumb, &dest).ok().map(|_| dest)
}

/// Small previews for a pack's `models`, from the pictures the pack ships with the same
/// names (Kenney: `Isometric/` or `Previews/`), cached as 128px PNGs.
pub(crate) fn pack_thumbs(source: &str, id: &str, models: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let (Some(first), Ok(dir)) = (models.first(), cache(source)) else { return vec![None; models.len()] };
    let pack = first.ancestors().find(|a| a.file_name().is_some_and(|n| n == "Models")).and_then(Path::parent);
    let mut pictures = std::collections::HashMap::new();
    for p in pack.map(walk).unwrap_or_default() {
        let folder = p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).unwrap_or("");
        if p.extension().is_some_and(|e| e == "png") && matches!(folder, "Isometric" | "Previews") {
            pictures.insert(p.file_stem().unwrap_or_default().to_os_string(), p);
        }
    }
    let out = dir.join(id).join("thumbs");
    let _ = fs::create_dir_all(&out);
    let one = |model: &PathBuf| -> Option<PathBuf> {
        let stem = model.file_stem()?;
        let dest = out.join(stem).with_extension("png");
        if dest.is_file() { return Some(dest); }
        let (w, h, px) = crate::png::decode(&fs::read(pictures.get(stem)?).ok()?)?;
        let (tw, th) = shrink(w, h, 128);
        fs::write(&dest, crate::png::encode_rgba(tw, th, &box_scale(&px, w, h, tw, th))).ok()?;
        Some(dest)
    };
    // ponytail: 8 threads over a few hundred pictures, once per pack; cached after that.
    std::thread::scope(|s| {
        let chunk = models.len().div_ceil(8).max(1);
        let jobs: Vec<_> = models.chunks(chunk).map(|c| s.spawn(move || c.iter().map(one).collect::<Vec<_>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    })
}

/// `w`x`h` scaled to fit in `max`, at least 1x1.
pub(super) fn shrink(w: usize, h: usize, max: usize) -> (usize, usize) {
    if w <= max && h <= max { return (w, h); }
    let s = max as f32 / w.max(h) as f32;
    (((w as f32 * s) as usize).max(1), ((h as f32 * s) as usize).max(1))
}

/// RGBA `px` (`w`x`h`) averaged down to `tw`x`th`.
pub(super) fn box_scale(px: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        let (y0, y1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    for c in 0..4 { sum[c] += px[(sy * w + sx) * 4 + c] as u32; }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend(sum.map(|v| (v / n) as u8));
        }
    }
    out
}
