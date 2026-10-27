//! `j3c -e imgdiff a.png b.png [--tol N] [--max N] [--out diff.png]`: golden compare.
//! Prints the number of pixels where any channel differs by more than `tol`; exits 1 when
//! that is above `max` (default 0) or the sizes differ. Same rule as `j3.gfx.canvas.diff`.

use crate::png;

pub fn cmd(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: j3c -e imgdiff a.png b.png [--tol N] [--max N] [--out diff.png]");
        println!("  Counts pixels where any channel differs by more than --tol (default 0, per channel 0-255).");
        println!("  Exit 0: count <= --max (default 0). Exit 1: more, or sizes differ. Exit 2: usage or unreadable PNG.");
        println!("  --out writes a diff image (changed pixels red). Capture frames with `j3c doc testing`.");
        return 0;
    }
    let (mut files, mut tol, mut max, mut out) = (Vec::new(), 0i32, 0usize, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tol" => tol = it.next().and_then(|v| v.parse().ok()).unwrap_or(-1),
            "--max" => max = it.next().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX),
            "--out" => out = it.next().cloned(),
            f => files.push(f.to_string()),
        }
    }
    if files.len() != 2 || tol < 0 {
        eprintln!("usage: j3c -e imgdiff a.png b.png [--tol N] [--max N] [--out diff.png]");
        return 2;
    }
    let (count, total) = match diff(&files[0], &files[1], tol, out.as_deref()) {
        Ok(r) => r,
        Err((rc, m)) => {
            if rc == 2 { eprintln!("{m}") } else { println!("{m}") }
            return rc;
        }
    };
    println!("{count} of {total} pixels differ (tol {tol})");
    if count > max { 1 } else { 0 }
}

/// Differing pixel count and total; `Err((exit code, message))` for unreadable files (2) or differing sizes (1).
pub fn diff(fa: &str, fb: &str, tol: i32, out: Option<&str>) -> Result<(usize, usize), (i32, String)> {
    let load = |p: &str| std::fs::read(p).ok().and_then(|b| png::decode(&b));
    let (Some((aw, ah, a)), Some((bw, bh, b))) = (load(fa), load(fb)) else {
        return Err((2, "imgdiff: could not read both PNGs".into()));
    };
    if (aw, ah) != (bw, bh) {
        return Err((1, format!("size differs: {aw}x{ah} vs {bw}x{bh}")));
    }
    let mut diff = vec![0u8; a.len()];
    let mut count = 0usize;
    for (i, (p, q)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        let bad = p.iter().zip(q).any(|(x, y)| (*x as i32 - *y as i32).abs() > tol);
        let o = &mut diff[i * 4..i * 4 + 4];
        if bad {
            count += 1;
            o.copy_from_slice(&[255, 0, 0, 255]);
        } else {
            let g = p[0] / 4;
            o.copy_from_slice(&[g, g, g, 255]);
        }
    }
    if let Some(path) = out {
        if std::fs::write(&path, png::encode_rgba(aw, ah, &diff)).is_err() {
            eprintln!("imgdiff: could not write {path}");
        }
    }
    Ok((count, aw * ah))
}
