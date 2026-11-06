//! Shrink the PNGs `j3.gfx.canvas` writes (unfiltered rows, fixed-Huffman LZ77) further before
//! handing them to an agent: per-row filters plus the same LZ77. Also PNG decoding (for baking textures at import) and
//! base64 for MCP image content. No crates.

/// `png` recompressed, or `None` when it isn't an 8-bit RGBA, non-interlaced PNG like
/// `Canvas.savePng` writes (then use it as is).
pub fn compact(png: &[u8]) -> Option<Vec<u8>> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" { return None; }
    let mut at = 8;
    let mut ihdr: Option<&[u8]> = None;
    let mut zdata = Vec::new();
    while at + 12 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let kind = &png[at + 4..at + 8];
        let data = png.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => ihdr = Some(data),
            b"IDAT" => zdata.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let ihdr = ihdr?;
    if ihdr.len() != 13 || ihdr[8] != 8 || ihdr[9] != 6 || ihdr[12] != 0 { return None; }
    let w = u32::from_be_bytes(ihdr[0..4].try_into().ok()?) as usize;
    let h = u32::from_be_bytes(ihdr[4..8].try_into().ok()?) as usize;
    let row = w * 4;
    let raw = inflate(zdata.get(2..)?, (row + 1).checked_mul(h)?)?;
    if raw.len() != (row + 1) * h { return None; }
    Some(write_png(ihdr, &refilter(&raw, row, h)?))
}

/// An 8-bit RGBA PNG of `rgba` (`w * h * 4` bytes), filtered and compressed like `compact`.
pub fn encode_rgba(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let row = w * 4;
    let mut raw = Vec::with_capacity((row + 1) * h);
    for line in rgba.chunks(row).take(h) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend((w as u32).to_be_bytes());
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    write_png(&ihdr, &refilter(&raw, row, h).unwrap_or(raw))
}

fn write_png(ihdr: &[u8], filtered: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    z.extend(deflate_fixed(filtered));
    z.extend(adler32(filtered).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Standard base64 with padding.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Raw DEFLATE (no zlib header): stored, fixed, and dynamic blocks, decoded the way zlib's
/// `contrib/puff` does. `canvas.decodePng` in J3 is the same algorithm; fix both together.
// ponytail: bit-at-a-time Huffman decode; add a 9-bit lookup table if it shows in a profile.
/// Raw DEFLATE; `None` past `cap` output bytes, so a tiny crafted file can't eat memory.
pub(crate) fn inflate(z: &[u8], cap: usize) -> Option<Vec<u8>> {
    struct In<'a> { z: &'a [u8], at: usize, buf: u32, cnt: u32 }
    impl In<'_> {
        fn bits(&mut self, n: u32) -> Option<u32> {
            while self.cnt < n {
                self.buf |= (*self.z.get(self.at)? as u32) << self.cnt;
                self.at += 1;
                self.cnt += 8;
            }
            let v = self.buf & ((1u64 << n) - 1) as u32;
            self.buf = if n == 32 { 0 } else { self.buf >> n };
            self.cnt -= n;
            Some(v)
        }
    }
    struct Huff { count: [u16; 16], symbol: Vec<u16> }
    fn build(lens: &[u8]) -> Option<Huff> {
        let mut count = [0u16; 16];
        for &l in lens { count[l as usize] += 1; }
        let mut left = 1i32;
        for len in 1..16 {
            left = left * 2 - count[len] as i32;
            if left < 0 { return None; }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 { offs[len + 1] = offs[len] + count[len]; }
        let mut symbol = vec![0u16; lens.len()];
        for (s, &l) in lens.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        count[0] = 0;
        Some(Huff { count, symbol })
    }
    fn decode(s: &mut In, h: &Huff) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= s.bits(1)? as i32;
            let count = h.count[len] as i32;
            if code - count < first { return h.symbol.get((index + code - first) as usize).copied(); }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let mut s = In { z, at: 0, buf: 0, cnt: 0 };
    let mut out = Vec::with_capacity(cap);
    loop {
        let last = s.bits(1)?;
        let kind = s.bits(2)?;
        if kind == 0 {
            s.buf = 0;
            s.cnt = 0;
            let len = u16::from_le_bytes(z.get(s.at..s.at + 2)?.try_into().ok()?) as usize;
            if out.len() + len > cap { return None; }
            out.extend_from_slice(z.get(s.at + 4..s.at + 4 + len)?);
            s.at += 4 + len;
        } else {
            let (lit, dist) = if kind == 1 {
                let mut l = [8u8; 288];
                l[144..256].fill(9);
                l[256..280].fill(7);
                (build(&l)?, build(&[5u8; 30])?)
            } else if kind == 2 {
                let nlen = s.bits(5)? as usize + 257;
                let ndist = s.bits(5)? as usize + 1;
                let ncode = s.bits(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for &o in &ORDER[..ncode] { cl[o] = s.bits(3)? as u8; }
                let ch = build(&cl)?;
                let mut lens = Vec::with_capacity(nlen + ndist);
                while lens.len() < nlen + ndist {
                    let sym = decode(&mut s, &ch)?;
                    let (v, n) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => (*lens.last()?, 3 + s.bits(2)? as usize),
                        17 => (0, 3 + s.bits(3)? as usize),
                        _ => (0, 11 + s.bits(7)? as usize),
                    };
                    lens.extend(std::iter::repeat_n(v, n));
                }
                if lens.len() != nlen + ndist || lens[256] == 0 { return None; }
                (build(&lens[..nlen])?, build(&lens[nlen..])?)
            } else {
                return None;
            };
            loop {
                let sym = decode(&mut s, &lit)? as usize;
                if sym < 256 {
                    if out.len() >= cap { return None; }
                    out.push(sym as u8);
                    continue;
                }
                if sym == 256 { break; }
                let li = sym - 257;
                if li >= 29 { return None; }
                let len = LEN_BASE[li] as usize + s.bits(LEN_EXTRA[li] as u32)? as usize;
                let di = decode(&mut s, &dist)? as usize;
                if di >= 30 { return None; }
                let d = DIST_BASE[di] as usize + s.bits(DIST_EXTRA[di] as u32)? as usize;
                if d > out.len() || out.len() + len > cap { return None; }
                for _ in 0..len { out.push(out[out.len() - d]); }
            }
        }
        if last == 1 { return Some(out); }
    }
}

/// RGBA8 pixels of an 8-bit, non-interlaced PNG (gray, RGB, palette, gray+alpha, RGBA),
/// or `None`. `canvas.decodePng` in J3 is the same algorithm; fix both together.
// ponytail: CRC and Adler aren't checked; 16-bit, interlaced, and low-bit-depth PNGs are None.
pub fn decode(png: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" { return None; }
    let mut at = 8;
    let (mut ihdr, mut plte, mut trns): (Option<&[u8]>, &[u8], &[u8]) = (None, &[], &[]);
    let mut zdata = Vec::new();
    while at + 12 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let data = png.get(at + 8..at + 8 + len)?;
        match &png[at + 4..at + 8] {
            b"IHDR" => ihdr = Some(data),
            b"PLTE" => plte = data,
            b"tRNS" => trns = data,
            b"IDAT" => zdata.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let ihdr = ihdr?;
    if ihdr.len() != 13 || ihdr[8] != 8 || ihdr[12] != 0 { return None; }
    let ctype = ihdr[9];
    let ch = match ctype { 0 => 1, 2 => 3, 3 => 1, 4 => 2, 6 => 4, _ => return None };
    let w = u32::from_be_bytes(ihdr[0..4].try_into().ok()?) as usize;
    let h = u32::from_be_bytes(ihdr[4..8].try_into().ok()?) as usize;
    let row = w.checked_mul(ch)?;
    let mut raw = inflate(zdata.get(2..)?, (row + 1).checked_mul(h)?)?;
    if raw.len() != h * (row + 1) { return None; }
    for y in 0..h {
        let base = y * (row + 1);
        let f = raw[base];
        for i in 0..row {
            let a = if i >= ch { raw[base + 1 + i - ch] as i32 } else { 0 };
            let b = if y > 0 { raw[base - row + i] as i32 } else { 0 };
            let c = if y > 0 && i >= ch { raw[base - row + i - ch] as i32 } else { 0 };
            let pred = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
                }
                _ => return None,
            };
            raw[base + 1 + i] = raw[base + 1 + i].wrapping_add(pred as u8);
        }
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let p = &raw[y * (row + 1) + 1 + x * ch..][..ch];
            match ctype {
                0 => out.extend([p[0], p[0], p[0], 255]),
                2 => out.extend([p[0], p[1], p[2], 255]),
                3 => {
                    let k = p[0] as usize;
                    out.extend_from_slice(plte.get(k * 3..k * 3 + 3)?);
                    out.push(*trns.get(k).unwrap_or(&255));
                }
                4 => out.extend([p[0], p[0], p[0], p[1]]),
                _ => out.extend_from_slice(p),
            }
        }
    }
    Some((w, h, out))
}

/// Rows with filter 0 (as canvas writes them) re-filtered with whichever of None, Sub, or
/// Up gives the smallest sum of absolute differences, the usual PNG heuristic.
fn refilter(raw: &[u8], row: usize, h: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(raw.len());
    let zero = vec![0u8; row];
    for y in 0..h {
        let line = &raw[y * (row + 1)..(y + 1) * (row + 1)];
        if line[0] != 0 { return None; }
        let cur = &line[1..];
        let up = if y == 0 { &zero[..] } else { &raw[(y - 1) * (row + 1) + 1..y * (row + 1)] };
        let sub: Vec<u8> = (0..row).map(|i| cur[i].wrapping_sub(if i >= 4 { cur[i - 4] } else { 0 })).collect();
        let upf: Vec<u8> = (0..row).map(|i| cur[i].wrapping_sub(up[i])).collect();
        let cost = |v: &[u8]| v.iter().map(|&b| (b as i8).unsigned_abs() as u64).sum::<u64>();
        let (kind, best) = [(0u8, cur.to_vec()), (1, sub), (2, upf)]
            .into_iter()
            .min_by_key(|(_, v)| cost(v))?;
        out.push(kind);
        out.extend(best);
    }
    Some(out)
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// `count` bits of `v`, least significant first.
    fn put(&mut self, v: u32, count: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, which DEFLATE stores most significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        self.put(code.reverse_bits() >> (32 - len), len);
    }

    fn literal(&mut self, sym: u32) {
        match sym {
            0..=143 => self.code(0x30 + sym, 8),
            144..=255 => self.code(0x190 + sym - 144, 9),
            256..=279 => self.code(sym - 256, 7),
            _ => self.code(0xC0 + sym - 280, 8),
        }
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// One fixed-Huffman block with greedy LZ77 over a 32 KiB window.
// ponytail: one hash candidate per position; hash chains or dynamic Huffman would shave more.
fn deflate_fixed(data: &[u8]) -> Vec<u8> {
    const WINDOW: usize = 32768;
    let mut bits = Bits { out: Vec::with_capacity(data.len() / 4), acc: 0, n: 0 };
    bits.put(1, 1);
    bits.put(1, 2);
    let mut head = vec![usize::MAX; 1 << 15];
    let hash = |i: usize| ((data[i] as usize) << 10 ^ (data[i + 1] as usize) << 5 ^ data[i + 2] as usize) & 0x7fff;
    let mut i = 0;
    while i < data.len() {
        let mut best = 0;
        let mut dist = 0;
        if i + 3 <= data.len() {
            let h = hash(i);
            let cand = head[h];
            head[h] = i;
            if cand != usize::MAX && i - cand <= WINDOW {
                let max = (data.len() - i).min(258);
                let mut l = 0;
                while l < max && data[cand + l] == data[i + l] { l += 1; }
                if l >= 3 {
                    best = l;
                    dist = i - cand;
                }
            }
        }
        if best == 0 {
            bits.literal(data[i] as u32);
            i += 1;
            continue;
        }
        let li = LEN_BASE.iter().rposition(|&b| b as usize <= best).unwrap();
        bits.literal(257 + li as u32);
        bits.put((best - LEN_BASE[li] as usize) as u32, LEN_EXTRA[li] as u32);
        let di = DIST_BASE.iter().rposition(|&b| b as usize <= dist).unwrap();
        bits.code(di as u32, 5);
        bits.put((dist - DIST_BASE[di] as usize) as u32, DIST_EXTRA[di] as u32);
        // Index the skipped positions so later matches can find them.
        for k in i + 1..(i + best).min(data.len().saturating_sub(2)) {
            head[hash(k)] = k;
        }
        i += best;
    }
    bits.literal(256);
    if bits.n > 0 { bits.put(0, 8 - bits.n); }
    bits.out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        for &byte in *part {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { 0xEDB88320 ^ (crc >> 1) } else { crc >> 1 };
            }
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend(crc32(&[kind, data]).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_rgba_round_trips() {
        let px: Vec<u8> = (0..5 * 3 * 4).map(|i| (i * 37 % 256) as u8).collect();
        let png = encode_rgba(5, 3, &px);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(decode(&png), Some((5, 3, px)));
    }

    #[test]
    fn deflate_round_trips() {
        let mut data = Vec::new();
        for i in 0..70000u32 { data.push(((i / 7) % 13) as u8 ^ (i % 251) as u8 / 64); }
        data.extend(std::iter::repeat(9u8).take(1000));
        data.extend(b"abcabcabcabd");
        assert_eq!(inflate(&deflate_fixed(&data), data.len()).unwrap(), data);
        assert_eq!(inflate(&deflate_fixed(&data), data.len() - 1), None);
        assert!(deflate_fixed(&data).len() < data.len() / 2);
    }

    #[test]
    fn decode_matches_fixtures() {
        for (png, rgba, w, h) in [
            (&include_bytes!("../examples/png_check/rgb.png")[..], &include_bytes!("../examples/png_check/rgb.rgba")[..], 37, 23),
            (include_bytes!("../examples/png_check/pal.png"), include_bytes!("../examples/png_check/pal.rgba"), 19, 17),
            (include_bytes!("../examples/png_check/tiles.png"), include_bytes!("../examples/png_check/tiles.rgba"), 64, 16),
            (include_bytes!("../examples/png_check/filt.png"), include_bytes!("../examples/png_check/filt.rgba"), 9, 10),
            // Hand-built: HDIST = 32 (legal, though zlib never writes more than 30).
            (include_bytes!("../examples/png_check/dist32.png"), include_bytes!("../examples/png_check/dist32.rgba"), 1, 1),
        ] {
            assert_eq!(decode(png), Some((w, h, rgba.to_vec())));
        }
        // An IHDR claiming fewer rows than the data holds: inflate stops at the cap.
        let mut short = include_bytes!("../examples/png_check/rgb.png").to_vec();
        short[16 + 4..16 + 8].copy_from_slice(&1u32.to_be_bytes());
        assert_eq!(decode(&short), None);
        // A stored RGBA PNG like canvas writes, then compacted (fixed Huffman, Sub/Up rows).
        let px: Vec<u8> = (0..3 * 2 * 4).map(|i| (i * 37 % 256) as u8).collect();
        let mut raw = Vec::new();
        for r in px.chunks(12) { raw.push(0); raw.extend_from_slice(r); }
        let mut z = vec![0x78, 0x01, 1];
        z.extend((raw.len() as u16).to_le_bytes());
        z.extend((!(raw.len() as u16)).to_le_bytes());
        z.extend(&raw);
        z.extend(adler32(&raw).to_be_bytes());
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut p, b"IHDR", &[0, 0, 0, 3, 0, 0, 0, 2, 8, 6, 0, 0, 0]);
        chunk(&mut p, b"IDAT", &z);
        chunk(&mut p, b"IEND", &[]);
        assert_eq!(decode(&p), Some((3, 2, px.clone())));
        assert_eq!(decode(&compact(&p).unwrap()), Some((3, 2, px)));
    }

    #[test]
    fn base64_matches_rfc() {
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"f"), "Zg==");
    }
}
