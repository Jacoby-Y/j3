//! Static glTF 2.0 importer for editor scenes. Converts primitives to j3mesh.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};

use crate::lsp::json::{self, Json};

mod material;
mod read;

use material::*;
use read::*;

fn vec3(accessor: &Accessor<'_>, i: usize) -> Result<[f32; 3], String> {
    if accessor.width != 3 || accessor.component != 5126 {
        return Err("glTF: expected float VEC3".into());
    }
    Ok([
        accessor.scalar(i, 0)?,
        accessor.scalar(i, 1)?,
        accessor.scalar(i, 2)?,
    ])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = cross(ab, ac);
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2])
        .sqrt()
        .max(0.000001);
    [n[0] / length, n[1] / length, n[2] / length]
}

#[derive(Clone)]
struct Part {
    name: String,
    parent: i64,
    kind: i64,
    asset: String,
    texture: String,
    double_sided: bool,
    alpha: i64,
    opacity: f32,
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
}

fn quat_euler(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let rx = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let sy = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0);
    let ry = sy.asin();
    let rz = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    [rx, ry, rz]
}

fn node_transform(node: &Json, warnings: &mut Vec<String>) -> Result<([f32; 3], [f32; 3], [f32; 3]), String> {
    if let Some(matrix) = node.get("matrix") {
        if arr(matrix).len() != 16 {
            return Err("glTF: node matrix must have 16 values".into());
        }
        let m = vector(Some(matrix), 16, 0.0);
        let mut x = [m[0], m[1], m[2]];
        let y = [m[4], m[5], m[6]];
        let z = [m[8], m[9], m[10]];
        let mut sx = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        let sy = (y[0] * y[0] + y[1] * y[1] + y[2] * y[2]).sqrt();
        let sz = (z[0] * z[0] + z[1] * z[1] + z[2] * z[2]).sqrt();
        if sx < 0.000001 || sy < 0.000001 || sz < 0.000001 {
            return Err("glTF: node matrix has a zero scale axis".into());
        }
        let handed = x[0] * (y[1] * z[2] - y[2] * z[1])
            + x[1] * (y[2] * z[0] - y[0] * z[2])
            + x[2] * (y[0] * z[1] - y[1] * z[0]);
        if handed < 0.0 {
            sx = -sx;
        }
        for v in &mut x {
            *v /= sx;
        }
        let y = [y[0] / sy, y[1] / sy, y[2] / sy];
        let z = [z[0] / sz, z[1] / sz, z[2] / sz];
        let dot_xy = x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
        let dot_xz = x[0] * z[0] + x[1] * z[1] + x[2] * z[2];
        let dot_yz = y[0] * z[0] + y[1] * z[1] + y[2] * z[2];
        if dot_xy.abs() > 0.001 || dot_xz.abs() > 0.001 || dot_yz.abs() > 0.001 {
            warn(warnings, "sheared node transforms were imported without the shear");
        }
        let (r00, r10, r20) = (x[0], x[1], x[2]);
        let (r01, r11, r21) = (y[0], y[1], y[2]);
        let (r02, r12, r22) = (z[0], z[1], z[2]);
        let q = if r00 + r11 + r22 > 0.0 {
            let s = (r00 + r11 + r22 + 1.0).sqrt() * 2.0;
            [(r21 - r12) / s, (r02 - r20) / s, (r10 - r01) / s, s * 0.25]
        } else if r00 > r11 && r00 > r22 {
            let s = (1.0 + r00 - r11 - r22).sqrt() * 2.0;
            [s * 0.25, (r01 + r10) / s, (r02 + r20) / s, (r21 - r12) / s]
        } else if r11 > r22 {
            let s = (1.0 + r11 - r00 - r22).sqrt() * 2.0;
            [(r01 + r10) / s, s * 0.25, (r12 + r21) / s, (r02 - r20) / s]
        } else {
            let s = (1.0 + r22 - r00 - r11).sqrt() * 2.0;
            [(r02 + r20) / s, (r12 + r21) / s, s * 0.25, (r10 - r01) / s]
        };
        return Ok(([m[12], m[13], m[14]], quat_euler(q), [sx, sy, sz]));
    }
    let pos = vector(node.get("translation"), 3, 0.0);
    let scale = vector(node.get("scale"), 3, 1.0);
    let q = vector(node.get("rotation"), 4, 0.0);
    let quat = if node.get("rotation").is_some() {
        [q[0], q[1], q[2], q[3]]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    Ok((
        [pos[0], pos[1], pos[2]],
        quat_euler(quat),
        [scale[0], scale[1], scale[2]],
    ))
}

/// What one import shares across its nodes.
struct Ctx<'a> {
    root: &'a Json,
    buffers: &'a [Vec<u8>],
    images: &'a [String],
    project: &'a Path,
    prefix: &'a str,
    /// Converted primitives (asset, texture, double sided) per glTF mesh, so a mesh
    /// used by several nodes is converted and written once.
    meshes: HashMap<usize, Vec<Look>>,
    seen: HashSet<usize>,
    warnings: Vec<String>,
    /// Bake textures into vertex colors (see `import`), with each image decoded once.
    bake: bool,
    baked: HashMap<String, Option<(usize, usize, Vec<u8>)>>,
}

/// A converted primitive: its mesh file and how it is drawn.
#[derive(Clone)]
struct Look {
    asset: String,
    texture: String,
    double_sided: bool,
    /// 0 opaque, 1 alpha mask (cut at 0.5), 2 alpha blend.
    alpha: i64,
    /// Base color alpha factor.
    opacity: f32,
}

fn convert_primitive(
    ctx: &mut Ctx<'_>,
    primitive: &Json,
    mi: usize,
    pi: usize,
) -> Result<Look, String> {
    let (root, buffers) = (ctx.root, ctx.buffers);
    if primitive.get("targets").is_some() {
        warn(&mut ctx.warnings, "morph targets were ignored");
    }
    if optional_usize(primitive, "mode").unwrap_or(4) != 4 {
        return Err("glTF: only triangle primitives are supported".into());
    }
    let attrs = field(primitive, "attributes")?;
    let positions = accessor(root, buffers, usize_field(attrs, "POSITION")?)?;
    let normals = optional_usize(attrs, "NORMAL")
        .map(|i| accessor(root, buffers, i))
        .transpose()?;
    let indices = optional_usize(primitive, "indices")
        .map(|i| accessor(root, buffers, i))
        .transpose()?;
    let count = indices.as_ref().map(|a| a.count).unwrap_or(positions.count);
    if count % 3 != 0 {
        return Err("glTF: triangle index count is not divisible by 3".into());
    }
    let (color, texture, uv_set, double_sided, alpha, opacity) =
        material(root, primitive, ctx.images, &mut ctx.warnings)?;
    // glTF color factors are linear; j3 shades in display (sRGB) colors like its textures.
    let color = [srgb(color[0]), srgb(color[1]), srgb(color[2])];
    let uvs = optional_usize(attrs, &format!("TEXCOORD_{uv_set}"))
        .map(|i| accessor(root, buffers, i))
        .transpose()?;
    if !texture.is_empty() && uvs.is_none() {
        return Err(format!("glTF: base color texture needs TEXCOORD_{uv_set}"));
    }
    let mut textured = uvs.is_some() && !texture.is_empty();
    let mut texture = texture;
    let mut image = None;
    if ctx.bake && textured {
        let project = ctx.project;
        let decoded = ctx
            .baked
            .entry(texture.clone())
            .or_insert_with(|| fs::read(project.join(&texture)).ok().and_then(|b| crate::png::decode(&b)));
        if decoded.is_some() {
            image = decoded.clone();
            textured = false;
            texture = String::new();
        } else {
            warn(&mut ctx.warnings, "bake needs 8-bit PNG textures; some kept their texture");
        }
    }
    if let Some(uv) = uvs.as_ref().filter(|_| textured || image.is_some()) {
        if uv.width != 2 || uv.component != 5126 {
            return Err("glTF: expected float UVs".into());
        }
    }
    let rel = format!("assets/imported/{}_mesh{mi}_prim{pi}.j3mesh", ctx.prefix);
    let mut out = format!("# j3mesh {}\n{count}\n", if textured { 2 } else { 1 });
    for tri in 0..count / 3 {
        let mut ids = [0usize; 3];
        let mut points = [[0f32; 3]; 3];
        for lane in 0..3 {
            ids[lane] = if let Some(idx) = &indices {
                idx.index(tri * 3 + lane)?
            } else {
                tri * 3 + lane
            };
            points[lane] = vec3(&positions, ids[lane])?;
        }
        let flat = normal(points[0], points[1], points[2]);
        // Baking: the texel at the triangle's UV centroid tints all three corners.
        let mut tint = [1f32; 3];
        if let (Some((w, h, px)), Some(uv)) = (&image, &uvs) {
            let (mut cu, mut cv) = (0f32, 0f32);
            for &id in &ids {
                cu += uv.scalar(id, 0)? / 3.0;
                cv += uv.scalar(id, 1)? / 3.0;
            }
            // glTF V points down the image, like rows, so there is no flip.
            let x = (((cu - cu.floor()) * *w as f32) as usize).min(w - 1);
            let y = (((cv - cv.floor()) * *h as f32) as usize).min(h - 1);
            let at = (y * w + x) * 4;
            tint = [px[at] as f32 / 255.0, px[at + 1] as f32 / 255.0, px[at + 2] as f32 / 255.0];
        }
        let color = [color[0] * tint[0], color[1] * tint[1], color[2] * tint[2]];
        for lane in 0..3 {
            let p = points[lane];
            let n = if let Some(normals) = &normals {
                vec3(normals, ids[lane])?
            } else {
                flat
            };
            out.push_str(&format!(
                "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} 0",
                p[0], p[1], p[2], n[0], n[1], n[2], color[0], color[1], color[2]
            ));
            if textured {
                let uv = uvs.as_ref().unwrap();
                out.push_str(&format!(
                    " {:.6} {:.6}",
                    uv.scalar(ids[lane], 0)?,
                    uv.scalar(ids[lane], 1)?
                ));
            }
            out.push('\n');
        }
    }
    fs::write(ctx.project.join(&rel), out).map_err(|e| e.to_string())?;
    Ok(Look { asset: rel, texture, double_sided, alpha, opacity })
}

fn append_node(
    ctx: &mut Ctx<'_>,
    node_index: usize,
    parent: i64,
    parts: &mut Vec<Part>,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("glTF: node hierarchy is too deep or cyclic".into());
    }
    let node = item(ctx.root, "nodes", node_index)?;
    if !ctx.seen.insert(node_index) {
        return Err(format!("glTF: node {node_index} has more than one parent"));
    }
    if node.get("skin").is_some() {
        warn(&mut ctx.warnings, "skinning was ignored");
    }
    let (position, rotation, scale) = node_transform(node, &mut ctx.warnings)?;
    let this = parts.len() as i64;
    parts.push(Part {
        name: optional_str(node, "name").unwrap_or("Group").into(),
        parent,
        kind: 0,
        asset: String::new(),
        texture: String::new(),
        double_sided: false,
        alpha: 0,
        opacity: 1.0,
        position,
        rotation,
        scale,
    });
    if let Some(mi) = optional_usize(node, "mesh") {
        let mesh = item(ctx.root, "meshes", mi)?;
        if !ctx.meshes.contains_key(&mi) {
            let mut converted = Vec::new();
            for (pi, primitive) in arr(field(mesh, "primitives")?).iter().enumerate() {
                // ponytail: any primitive that can't convert is skipped, malformed or not;
                // file-level problems still fail the whole import.
                match convert_primitive(ctx, primitive, mi, pi) {
                    Ok(done) => converted.push(done),
                    Err(e) => warn(&mut ctx.warnings, e.trim_start_matches("glTF: ")),
                }
            }
            ctx.meshes.insert(mi, converted);
        }
        for look in &ctx.meshes[&mi] {
            parts.push(Part {
                name: optional_str(mesh, "name").unwrap_or("Mesh").into(),
                parent: this,
                kind: 1,
                asset: look.asset.clone(),
                texture: look.texture.clone(),
                double_sided: look.double_sided,
                alpha: look.alpha,
                opacity: look.opacity,
                position: [0.0; 3],
                rotation: [0.0; 3],
                scale: [1.0; 3],
            });
        }
    }
    if let Some(children) = node.get("children") {
        for child in arr(children) {
            let index = child
                .as_i64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("glTF: invalid child index")?;
            append_node(ctx, index, this, parts, depth + 1)?;
        }
    }
    Ok(())
}

/// Write `parts` as a standalone scene file at `out`, parents before children, ids from 1.
/// The editor pastes it into the open scene, so the scene file itself is never touched.
fn write_scene(out: &Path, parts: &[Part]) -> Result<(), String> {
    let objects: Vec<crate::scenefile::Object> = parts
        .iter()
        .enumerate()
        .map(|(p, part)| {
            let mut o = crate::scenefile::Object::new(p);
            o.id = p as i64 + 1;
            o.parent = if part.parent < 0 { 0 } else { part.parent + 1 };
            o.kind = part.kind;
            o.name = part.name.replace(['\n', '\r'], " ");
            o.asset = part.asset.clone();
            o.texture = part.texture.clone();
            o.double_sided = part.double_sided;
            o.alpha = part.alpha;
            o.opacity = part.opacity as f64;
            o.pos = part.position.map(f64::from);
            o.rot = part.rotation.map(f64::from);
            o.scale = part.scale.map(f64::from);
            o
        })
        .collect();
    fs::write(out, crate::scenefile::write_v2(&objects, parts.len() as i64 + 1)).map_err(|e| e.to_string())
}

/// Convert a glTF file and write its hierarchy to the scene file `scene`.
/// Returns warnings about unsupported features that were skipped.
/// `bake` samples each textured triangle's texture at its UV centroid into vertex colors and
/// drops the texture (palette kits like Kenney's), for 8-bit PNG textures.
pub fn import(project: &Path, scene: &Path, source: &Path, bake: bool) -> Result<Vec<String>, String> {
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
    if extension != "gltf" && extension != "glb" {
        return Err("expected .gltf or .glb".into());
    }
    let (root, bin) = read_source(source)?;
    let buffers = buffers(&root, source, bin)?;
    let prefix = unique_prefix(project, source)?;
    let result = convert(&root, &buffers, project, scene, source, &prefix, bake);
    if result.is_err() {
        // Every file this import wrote starts with `<prefix>_`, and none did before it.
        let folder = project.join("assets/imported");
        for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with(&format!("{prefix}_")) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    result
}

fn convert(
    root: &Json,
    buffers: &[Vec<u8>],
    project: &Path,
    scene: &Path,
    source: &Path,
    prefix: &str,
    bake: bool,
) -> Result<Vec<String>, String> {
    for extension in root.get("extensionsRequired").map(arr).unwrap_or(&[]) {
        let name = extension.as_str().ok_or("glTF: invalid required extension")?;
        return Err(format!("glTF: needs {name}; re-export without compression"));
    }
    let mut warnings = Vec::new();
    if root
        .get("animations")
        .map(arr)
        .is_some_and(|xs| !xs.is_empty())
    {
        warn(&mut warnings, "animations were ignored");
    }
    let images = write_images(root, source, buffers, project, prefix, &mut warnings)?;
    let mut ctx = Ctx {
        root,
        buffers,
        images: &images,
        project,
        prefix,
        meshes: HashMap::new(),
        seen: HashSet::new(),
        warnings,
        bake,
        baked: HashMap::new(),
    };
    let scene_index = optional_usize(root, "scene").unwrap_or(0);
    let scene_spec = item(root, "scenes", scene_index)?;
    let mut parts = vec![Part {
        name: safe_stem(source),
        parent: -1,
        kind: 0,
        asset: String::new(),
        texture: String::new(),
        double_sided: false,
        alpha: 0,
        opacity: 1.0,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: [1.0; 3],
    }];
    for node in arr(field(scene_spec, "nodes")?) {
        let index = node
            .as_i64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("glTF: invalid root node")?;
        append_node(&mut ctx, index, 0, &mut parts, 0)?;
    }
    write_scene(scene, &parts)?;
    Ok(ctx.warnings)
}

/// Copy an existing native mesh into the project and place it in the scene.
pub fn import_mesh(project: &Path, scene: &Path, source: &Path) -> Result<Vec<String>, String> {
    let bytes = fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if !bytes.starts_with(b"# j3mesh ") {
        return Err("j3mesh: expected a `# j3mesh 1` or `# j3mesh 2` header".into());
    }
    let prefix = unique_prefix(project, source)?;
    let rel = format!("assets/imported/{prefix}_mesh0_prim0.j3mesh");
    fs::write(project.join(&rel), bytes).map_err(|e| e.to_string())?;
    write_scene(
        scene,
        &[Part {
            name: safe_stem(source),
            parent: -1,
            kind: 1,
            asset: rel,
            texture: String::new(),
            double_sided: false,
            alpha: 0,
            opacity: 1.0,
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }],
    )?;
    Ok(Vec::new())
}

#[cfg(test)]
mod tests;
