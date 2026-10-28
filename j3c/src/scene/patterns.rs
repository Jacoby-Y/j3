pub(super) const SHAPES: &[&str] = &["cube", "sphere", "cylinder", "cone", "capsule", "torus", "quad", "plane", "sky"];

/// The start of an `add` request for `what`: a shape, a mesh asset, or a scene placed as an instance.
pub(crate) fn add_what(what: &str) -> Result<Vec<String>, String> {
    let mut req = vec!["add".to_string()];
    if what.ends_with(".j3scene") {
        req.extend(["instance".into(), format!("scene={what}")]);
    } else if what.ends_with(".j3mesh") || what.starts_with("builtin:") {
        req.extend(["mesh".into(), format!("asset={what}")]);
    } else if SHAPES.contains(&what) {
        req.push(what.into());
    } else {
        return Err(format!("`{what}`: expected a .j3scene, .j3mesh, builtin: asset, or shape"));
    }
    Ok(req)
}

pub(super) const PATTERNS: &[&str] = &["ring", "stairs", "spiral", "wall"];

/// `ring`, `stairs`, `spiral`, and `wall`: the `add` requests that build the shape, run as one
/// batch. `kv` are `key=value` options. Angles are degrees; rotation about +y by `a` sends
/// local +x to `(cos a, 0, -sin a)`, so a piece whose local x points along `(dx, dz)` has
/// yaw `-atan2(dz, dx)`.
// ponytail: sugar over `add` like grid; pieces go under `parent`, not a new group, since a
// batch can't name an object it hasn't made yet.
pub(crate) fn pattern_requests(op: &str, kv: &[String]) -> Result<Vec<Vec<String>>, String> {
    let mut opts = std::collections::HashMap::new();
    for a in kv {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("{a}: expected key=value"))?;
        opts.insert(k.to_string(), v.to_string());
    }
    let nums = |key: &str, want: usize, default: Option<&[f64]>| -> Result<Vec<f64>, String> {
        match opts.get(key) {
            Some(v) => {
                let n: Vec<f64> = v.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>()
                    .map_err(|_| format!("{key}: expected {want} number(s)"))?;
                if n.len() != want { return Err(format!("{key}: expected {want} number(s), got {}", n.len())); }
                Ok(n)
            }
            None => default.map(<[f64]>::to_vec).ok_or_else(|| format!("{op} needs {key}")),
        }
    };
    let one = |key: &str, default: Option<f64>| -> Result<f64, String> {
        Ok(nums(key, 1, default.as_ref().map(std::slice::from_ref))?[0])
    };
    let what = opts.get("what").cloned().unwrap_or_else(|| "cube".into());
    let name = opts.get("name").cloned();
    let parent = opts.get("parent").cloned();
    let f = |x: f64| {
        let r = (x * 1000.0).round() / 1000.0;
        if r == 0.0 { "0".to_string() } else { format!("{r}") }
    };
    let piece = |pos: [f64; 3], yaw_deg: f64, size: [f64; 3], default_name: &str| -> Result<Vec<String>, String> {
        let mut req = add_what(&what)?;
        req.push(format!("pos={},{},{}", f(pos[0]), f(pos[1]), f(pos[2])));
        req.push(format!("rot=0,{},0", f(yaw_deg)));
        req.push(format!("scale={},{},{}", f(size[0]), f(size[1]), f(size[2])));
        req.push(format!("name={}", name.clone().unwrap_or_else(|| default_name.into())));
        if let Some(p) = &parent { req.push(format!("parent={p}")); }
        Ok(req)
    };
    let known: &[&str] = match op {
        "ring" => &["center", "radius", "count", "y", "size", "face", "arc"],
        "stairs" => &["from", "dir", "steps", "rise", "run", "width", "solid"],
        "spiral" => &["center", "radius", "steps", "rise", "turn", "start", "thickness", "width", "depth", "y"],
        "wall" => &["from", "to", "height", "thickness", "door", "y"],
        _ => return Err(format!("unknown pattern {op}; expected one of {}", PATTERNS.join(", "))),
    };
    if let Some(bad) = opts.keys().find(|k| !known.contains(&k.as_str()) && !matches!(k.as_str(), "what" | "name" | "parent")) {
        return Err(format!("{op}: unknown option {bad}; options: {}, what, name, parent", known.join(", ")));
    }
    let mut reqs = Vec::new();
    match op {
        "ring" => {
            let c = nums("center", 2, Some(&[0.0, 0.0]))?;
            let r = one("radius", None)?;
            let n = one("count", None)?;
            if n < 1.0 || n > 2000.0 || n.fract() != 0.0 { return Err("count: a whole number from 1 to 2000".into()); }
            let n = n as usize;
            let y = one("y", Some(0.0))?;
            let face = opts.get("face").map(String::as_str).unwrap_or("tangent");
            let (a0, a1, full) = match opts.get("arc") {
                Some(_) => { let a = nums("arc", 2, None)?; (a[0], a[1], false) }
                None => (0.0, 360.0, true),
            };
            let step = if full { (a1 - a0) / n as f64 } else if n > 1 { (a1 - a0) / (n - 1) as f64 } else { 0.0 };
            let mut size = nums("size", 3, Some(&[1.0, 3.0, 0.0]))?;
            if face == "tangent" && size[2] == 0.0 {
                // Neighbours overlap at the outer edge, so the wall has no slits.
                size[2] = 2.0 * (r + size[0]) * (step.abs().to_radians() / 2.0).tan();
            }
            if size[2] == 0.0 { size[2] = 1.0; }
            for i in 0..n {
                let t = a0 + step * i as f64;
                let tr = t.to_radians();
                let pos = [c[0] + r * tr.cos(), y + size[1] / 2.0, c[1] + r * tr.sin()];
                // Local +x outward for tangent; local +z outward (out) or inward (in).
                let yaw = match face {
                    "tangent" => -t,
                    "out" => 90.0 - t,
                    "in" => -90.0 - t,
                    other => return Err(format!("face {other}: expected tangent, out, or in")),
                };
                reqs.push(piece(pos, yaw, [size[0], size[1], size[2]], "RingPiece")?);
            }
        }
        "stairs" => {
            let from = nums("from", 3, None)?;
            let (dx, dz) = match opts.get("dir").map(String::as_str).unwrap_or("z") {
                "x" => (1.0, 0.0),
                "-x" => (-1.0, 0.0),
                "z" => (0.0, 1.0),
                "-z" => (0.0, -1.0),
                other => return Err(format!("dir {other}: expected x, -x, z, or -z")),
            };
            let steps = one("steps", None)?;
            if steps < 1.0 || steps > 500.0 || steps.fract() != 0.0 { return Err("steps: a whole number from 1 to 500".into()); }
            let rise = one("rise", Some(0.4))?;
            let run = one("run", Some(1.0))?;
            let width = one("width", Some(2.5))?;
            let solid = opts.get("solid").map(|v| v == "true").unwrap_or(true);
            for i in 0..steps as usize {
                let top = from[1] + rise * (i + 1) as f64;
                let h = if solid { rise * (i + 1) as f64 } else { rise };
                let along = run * (i as f64 + 0.5);
                let pos = [from[0] + dx * along, top - h / 2.0, from[2] + dz * along];
                let size = if dx != 0.0 { [run, h, width] } else { [width, h, run] };
                reqs.push(piece(pos, 0.0, size, "Stair")?);
            }
        }
        "spiral" => {
            let c = nums("center", 2, Some(&[0.0, 0.0]))?;
            let r = one("radius", None)?;
            let steps = one("steps", None)?;
            if steps < 1.0 || steps > 500.0 || steps.fract() != 0.0 { return Err("steps: a whole number from 1 to 500".into()); }
            let rise = one("rise", Some(0.4))?;
            let turn = one("turn", Some(20.0))?;
            let start = one("start", Some(0.0))?;
            let thick = one("thickness", Some(0.6))?;
            let width = one("width", Some(2.0))?;
            let depth = one("depth", Some(1.2))?;
            let y = one("y", Some(0.0))?;
            for i in 0..steps as usize {
                let t = start + turn * i as f64;
                let tr = t.to_radians();
                let top = y + rise * (i + 1) as f64;
                let pos = [c[0] + r * tr.cos(), top - thick / 2.0, c[1] + r * tr.sin()];
                reqs.push(piece(pos, -t, [width, thick, depth], "SpiralStep")?);
            }
        }
        "wall" => {
            let a = nums("from", 2, None)?;
            let b = nums("to", 2, None)?;
            let h = one("height", Some(3.0))?;
            let t = one("thickness", Some(1.5))?;
            let y = one("y", Some(0.0))?;
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 { return Err("wall: from and to are the same point".into()); }
            let yaw = -dz.atan2(dx).to_degrees();
            let (ux, uz) = (dx / len, dz / len);
            // A run along the wall from s0 to s1, bottom b0 to top b1.
            let mut seg = |s0: f64, s1: f64, b0: f64, b1: f64| -> Result<(), String> {
                if s1 - s0 < 0.001 || b1 - b0 < 0.001 { return Ok(()); }
                let m = (s0 + s1) / 2.0;
                reqs.push(piece([a[0] + ux * m, y + (b0 + b1) / 2.0, a[1] + uz * m], yaw, [s1 - s0, b1 - b0, t], "Wall")?);
                Ok(())
            };
            match opts.get("door") {
                None => seg(0.0, len, 0.0, h)?,
                Some(_) => {
                    let d = nums("door", 3, None)?;
                    let (dw, dh, at) = (d[0], d[1], d[2]);
                    if at - dw / 2.0 < 0.0 || at + dw / 2.0 > len || dh > h {
                        return Err(format!("door {dw} wide, {dh} high at {at} does not fit a wall {len:.2} long and {h} high"));
                    }
                    seg(0.0, at - dw / 2.0, 0.0, h)?;
                    seg(at + dw / 2.0, len, 0.0, h)?;
                    seg(at - dw / 2.0, at + dw / 2.0, dh, h)?;
                }
            }
        }
        _ => unreachable!(),
    }
    if reqs.len() > 2000 { return Err(format!("{op} makes {} pieces; at most 2000", reqs.len())); }
    Ok(reqs)
}

#[cfg(test)]
mod pattern_tests {
    use super::*;

    fn kv(s: &str) -> Vec<String> { s.split(' ').map(str::to_string).collect() }
    fn opt<'a>(req: &'a [String], key: &str) -> &'a str {
        req.iter().find_map(|a| a.strip_prefix(&format!("{key}="))).unwrap_or("")
    }

    #[test]
    fn ring_tangent_slabs_close_the_circle() {
        let reqs = pattern_requests("ring", &kv("radius=8.5 count=12 size=1.2,6,0")).unwrap();
        assert_eq!(reqs.len(), 12);
        // The silo that needed 5.1 m slabs at r 8.5, 12 of them, 1.2 thick.
        let width: f64 = opt(&reqs[0], "scale").split(',').nth(2).unwrap().parse().unwrap();
        assert!((width - 5.198).abs() < 0.01, "{width}");
        assert_eq!(opt(&reqs[0], "pos"), "8.5,3,0");
        assert_eq!(opt(&reqs[3], "rot"), "0,-90,0");
    }

    #[test]
    fn stairs_and_door_wall() {
        let reqs = pattern_requests("stairs", &kv("from=0,0,0 dir=-x steps=3 rise=0.4 run=1 width=2")).unwrap();
        assert_eq!(opt(&reqs[2], "pos"), "-2.5,0.6,0");
        assert_eq!(opt(&reqs[2], "scale"), "1,1.2,2");
        let reqs = pattern_requests("wall", &kv("from=0,0 to=0,10 height=4 thickness=1 door=2,3,5")).unwrap();
        assert_eq!(reqs.len(), 3);
        assert_eq!(opt(&reqs[0], "rot"), "0,-90,0");
        assert_eq!(opt(&reqs[2], "pos"), "0,3.5,5");
        assert!(pattern_requests("wall", &kv("from=0,0 to=0,10 door=2,3,9.5")).is_err());
        assert!(pattern_requests("ring", &kv("radius=2 count=3 bogus=1")).is_err());
    }
}
