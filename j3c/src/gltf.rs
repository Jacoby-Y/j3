//! Static glTF 2.0 importer for editor scenes. Converts primitives to j3mesh.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};

use crate::lsp::json::{self, Json};

/// Linear 0-1 to sRGB 0-1.
fn srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn arr(value: &Json) -> &[Json] {
    match value {
        Json::Arr(xs) => xs,
        _ => &[],
    }
}

fn field<'a>(value: &'a Json, name: &str) -> Result<&'a Json, String> {
    value
        .get(name)
        .ok_or_else(|| format!("glTF: missing `{name}`"))
}

fn usize_field(value: &Json, name: &str) -> Result<usize, String> {
    field(value, name)?
        .as_i64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("glTF: invalid `{name}`"))
}

fn optional_usize(value: &Json, name: &str) -> Option<usize> {
    value
        .get(name)?
        .as_i64()
        .and_then(|n| usize::try_from(n).ok())
}

fn optional_str<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str()
}

fn item<'a>(root: &'a Json, table: &str, index: usize) -> Result<&'a Json, String> {
    arr(field(root, table)?)
        .get(index)
        .ok_or_else(|| format!("glTF: {table}[{index}] is missing"))
}

fn f32_at(value: &Json, index: usize, default: f32) -> f32 {
    arr(value)
        .get(index)
        .and_then(Json::as_f64)
        .map(|n| n as f32)
        .unwrap_or(default)
}

fn vector(value: Option<&Json>, len: usize, default: f32) -> Vec<f32> {
    (0..len)
        .map(|i| value.map(|v| f32_at(v, i, default)).unwrap_or(default))
        .collect()
}

/// Note an unsupported feature that was skipped. Each distinct message is kept once.
fn warn(warnings: &mut Vec<String>, message: &str) {
    if !warnings.iter().any(|w| w == message) {
        warnings.push(message.into());
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes.get(offset..offset + 4).ok_or("glTF: truncated GLB")?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}

fn read_source(path: &Path) -> Result<(Json, Option<Vec<u8>>), String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path.extension().and_then(|s| s.to_str()) == Some("glb") {
        if bytes.len() < 20 || &bytes[0..4] != b"glTF" || read_u32(&bytes, 4)? != 2 {
            return Err("glTF: unsupported GLB header".into());
        }
        let total = read_u32(&bytes, 8)? as usize;
        if total > bytes.len() {
            return Err("glTF: truncated GLB".into());
        }
        let mut at = 12;
        let mut json_bytes = None;
        let mut bin = None;
        while at + 8 <= total {
            let len = read_u32(&bytes, at)? as usize;
            let kind = read_u32(&bytes, at + 4)?;
            at += 8;
            let chunk = bytes.get(at..at + len).ok_or("glTF: truncated GLB chunk")?;
            if kind == 0x4e4f534a {
                json_bytes = Some(chunk.to_vec());
            }
            if kind == 0x004e4942 {
                bin = Some(chunk.to_vec());
            }
            at += len;
        }
        let json_bytes = json_bytes.ok_or("glTF: GLB has no JSON chunk")?;
        let text = std::str::from_utf8(&json_bytes).map_err(|e| e.to_string())?;
        Ok((json::parse(text.trim_end_matches('\0')).map_err(|e| format!("{}: {e}", path.display()))?, bin))
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        Ok((json::parse(text).map_err(|e| format!("{}: {e}", path.display()))?, None))
    }
}

fn decode64(encoded: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut held = 0u32;
    for b in encoded.bytes() {
        if b == b'=' {
            break;
        }
        let value = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\r' | b'\n' => continue,
            _ => return Err("glTF: invalid base64 data URI".into()),
        };
        bits = (bits << 6) | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    Ok(out)
}

fn uri_bytes(base: &Path, uri: &str) -> Result<Vec<u8>, String> {
    if let Some((_, data)) = uri.split_once(";base64,") {
        return decode64(data);
    }
    let mut decoded = Vec::with_capacity(uri.len());
    let mut bytes = uri.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let digit = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
            let hi = bytes.next().and_then(digit).ok_or("glTF: invalid URI escape")?;
            let lo = bytes.next().and_then(digit).ok_or("glTF: invalid URI escape")?;
            decoded.push((hi << 4) | lo);
        } else {
            decoded.push(b);
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| "glTF: URI is not UTF-8")?;
    let path = Path::new(&decoded);
    if decoded.contains("://")
        || decoded.contains(':')
        || decoded.contains('\\')
        || decoded.contains('\0')
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir))
    {
        return Err(format!("glTF: unsupported or unsafe URI `{uri}`"));
    }
    fs::read(base.join(path)).map_err(|e| format!("glTF asset `{uri}`: {e}"))
}

fn buffers(root: &Json, path: &Path, bin: Option<Vec<u8>>) -> Result<Vec<Vec<u8>>, String> {
    let mut result = Vec::new();
    for (i, spec) in arr(field(root, "buffers")?).iter().enumerate() {
        let data = if let Some(uri) = optional_str(spec, "uri") {
            uri_bytes(path.parent().unwrap_or(Path::new(".")), uri)?
        } else if i == 0 {
            bin.clone().ok_or("glTF: missing GLB BIN chunk")?
        } else {
            return Err(format!("glTF: buffer {i} has no URI"));
        };
        let expected = usize_field(spec, "byteLength")?;
        if data.len() < expected {
            return Err(format!("glTF: buffer {i} is truncated"));
        }
        result.push(data);
    }
    Ok(result)
}

struct Accessor<'a> {
    bytes: &'a [u8],
    start: usize,
    stride: usize,
    count: usize,
    component: usize,
    width: usize,
}

fn accessor<'a>(root: &Json, buffers: &'a [Vec<u8>], index: usize) -> Result<Accessor<'a>, String> {
    let spec = item(root, "accessors", index)?;
    if spec.get("sparse").is_some() {
        return Err("glTF: sparse accessors are unsupported".into());
    }
    let view = item(root, "bufferViews", usize_field(spec, "bufferView")?)?;
    let buffer = buffers
        .get(usize_field(view, "buffer")?)
        .ok_or("glTF: bufferView references a missing buffer")?;
    let component = usize_field(spec, "componentType")?;
    let component_bytes = match component {
        5121 => 1,
        5123 => 2,
        5125 | 5126 => 4,
        _ => return Err(format!("glTF: component type {component} is unsupported")),
    };
    let width = match optional_str(spec, "type").unwrap_or("") {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        other => return Err(format!("glTF: accessor type `{other}` is unsupported")),
    };
    let start = optional_usize(view, "byteOffset").unwrap_or(0)
        + optional_usize(spec, "byteOffset").unwrap_or(0);
    let stride = optional_usize(view, "byteStride").unwrap_or(width * component_bytes);
    let count = usize_field(spec, "count")?;
    if stride < width * component_bytes {
        return Err("glTF: invalid buffer stride".into());
    }
    if count > 0 && start + (count - 1) * stride + width * component_bytes > buffer.len() {
        return Err("glTF: accessor exceeds buffer bounds".into());
    }
    Ok(Accessor {
        bytes: buffer,
        start,
        stride,
        count,
        component,
        width,
    })
}

impl Accessor<'_> {
    fn scalar(&self, index: usize, lane: usize) -> Result<f32, String> {
        if index >= self.count || lane >= self.width {
            return Err("glTF: accessor index out of bounds".into());
        }
        let size = match self.component {
            5121 => 1,
            5123 => 2,
            _ => 4,
        };
        let at = self.start + index * self.stride + lane * size;
        let bytes = &self.bytes[at..at + size];
        Ok(match self.component {
            5121 => f32::from(bytes[0]),
            5123 => f32::from(u16::from_le_bytes(bytes.try_into().unwrap())),
            5125 => u32::from_le_bytes(bytes.try_into().unwrap()) as f32,
            5126 => f32::from_le_bytes(bytes.try_into().unwrap()),
            _ => unreachable!(),
        })
    }

    fn index(&self, index: usize) -> Result<usize, String> {
        if self.width != 1 || self.component == 5126 {
            return Err("glTF: index accessor must be an integer SCALAR".into());
        }
        if index >= self.count {
            return Err("glTF: index accessor out of bounds".into());
        }
        let at = self.start + index * self.stride;
        Ok(match self.component {
            5121 => usize::from(self.bytes[at]),
            5123 => usize::from(u16::from_le_bytes(
                self.bytes[at..at + 2].try_into().unwrap(),
            )),
            5125 => u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap()) as usize,
            _ => unreachable!(),
        })
    }
}

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

fn safe_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let text: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if text.is_empty() {
        "model".into()
    } else {
        text
    }
}

fn unique_prefix(project: &Path, path: &Path) -> Result<String, String> {
    let stem = safe_stem(path);
    let folder = project.join("assets/imported");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let existing: Vec<String> = fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().and_then(|e| e.file_name().into_string().ok()))
        .collect();
    for n in 0..10000 {
        let prefix = if n == 0 {
            stem.clone()
        } else {
            format!("{stem}_{n}")
        };
        if !existing
            .iter()
            .any(|name| name.starts_with(&format!("{prefix}_")))
        {
            return Ok(prefix);
        }
    }
    Err("glTF: could not allocate an import name".into())
}

fn write_images(
    root: &Json,
    path: &Path,
    buffers: &[Vec<u8>],
    project: &Path,
    prefix: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut needed = HashSet::new();
    for material in root.get("materials").map(arr).unwrap_or(&[]) {
        if let Some(info) = material
            .get("pbrMetallicRoughness")
            .and_then(|pbr| pbr.get("baseColorTexture"))
        {
            let texture = item(root, "textures", usize_field(info, "index")?)?;
            needed.insert(usize_field(texture, "source")?);
        }
    }
    let images = root.get("images").map(arr).unwrap_or(&[]);
    for (i, image) in images.iter().enumerate() {
        if !needed.contains(&i) {
            result.push(String::new());
            continue;
        }
        let data = if let Some(uri) = optional_str(image, "uri") {
            match uri_bytes(path.parent().unwrap_or(Path::new(".")), uri) {
                Ok(bytes) => bytes,
                Err(_) => {
                    warn(warnings, "some textures could not be read");
                    result.push(String::new());
                    continue;
                }
            }
        } else if let Some(view_index) = optional_usize(image, "bufferView") {
            let view = item(root, "bufferViews", view_index)?;
            let bytes = buffers
                .get(usize_field(view, "buffer")?)
                .ok_or("glTF: image buffer missing")?;
            let offset = optional_usize(view, "byteOffset").unwrap_or(0);
            let len = usize_field(view, "byteLength")?;
            bytes
                .get(offset..offset + len)
                .ok_or("glTF: image data truncated")?
                .to_vec()
        } else {
            return Err(format!("glTF: image {i} has no data"));
        };
        let extension = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else if data.starts_with(&[0xff, 0xd8]) {
            "jpg"
        } else {
            warn(warnings, "images that aren't PNG or JPEG were skipped");
            result.push(String::new());
            continue;
        };
        let rel = format!("assets/imported/{prefix}_image{i}.{extension}");
        fs::write(project.join(&rel), data).map_err(|e| e.to_string())?;
        result.push(rel);
    }
    Ok(result)
}

fn material(
    root: &Json,
    primitive: &Json,
    images: &[String],
    warnings: &mut Vec<String>,
) -> Result<([f32; 3], String, usize, bool, i64, f32), String> {
    let Some(index) = optional_usize(primitive, "material") else {
        return Ok(([1.0; 3], String::new(), 0, false, 0, 1.0));
    };
    let mat = item(root, "materials", index)?;
    let alpha = match optional_str(mat, "alphaMode").unwrap_or("OPAQUE") {
        "MASK" => 1,
        "BLEND" => 2,
        _ => 0,
    };
    if alpha == 1 && mat.get("alphaCutoff").and_then(|c| c.as_f64()).is_some_and(|c| (c - 0.5).abs() > 0.001) {
        warn(warnings, "alpha cutoffs other than 0.5 were imported as 0.5");
    }
    let double_sided = matches!(mat.get("doubleSided"), Some(Json::Bool(true)));
    let pbr = mat.get("pbrMetallicRoughness").unwrap_or(&Json::Null);
    // ponytail: the renderer has no tangents or PBR maps; see "Rendering gaps" in EDITOR.md.
    if mat.get("normalTexture").or(mat.get("occlusionTexture")).or(pbr.get("metallicRoughnessTexture")).is_some() {
        warn(warnings, "normal, metal/roughness, and occlusion maps were ignored");
    }
    let factor = pbr
        .get("baseColorFactor")
        .map(|v| vector(Some(v), 4, 1.0))
        .unwrap_or(vec![1.0; 4]);
    let mut uv_set = 0;
    let texture = if let Some(info) = pbr.get("baseColorTexture") {
        uv_set = optional_usize(info, "texCoord").unwrap_or(0);
        // Only offset, rotation, or scale change the UVs; a bare texCoord doesn't.
        let transform = info.get("extensions").and_then(|e| e.get("KHR_texture_transform"));
        if transform.is_some_and(|t| ["offset", "rotation", "scale"].iter().any(|k| t.get(k).is_some())) {
            warn(warnings, "texture transforms were ignored");
        }
        let tex = item(root, "textures", usize_field(info, "index")?)?;
        let source = usize_field(tex, "source")?;
        images
            .get(source)
            .cloned()
            .ok_or("glTF: texture image missing")?
    } else {
        String::new()
    };
    Ok((
        [factor[0], factor[1], factor[2]],
        texture,
        uv_set,
        double_sided,
        alpha,
        factor[3],
    ))
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

fn safe_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

/// Write `parts` as a standalone scene file at `out`, parents before children, ids from 1.
/// The editor pastes it into the open scene, so the scene file itself is never touched.
fn write_scene(out: &Path, parts: &[Part]) -> Result<(), String> {
    let mut text = format!("scene.version=1\nscene.count={}\nscene.nextId={}\n", parts.len(), parts.len() + 1);
    for (p, part) in parts.iter().enumerate() {
        let id = p + 1;
        let parent = if part.parent < 0 { 0 } else { part.parent as usize + 1 };
        let prefix = format!("node.{p}.");
        text.push_str(&format!("{prefix}id={id}\n{prefix}parent={parent}\n{prefix}kind={}\n{prefix}name s={}\n{prefix}asset s={}\n{prefix}texture s={}\n{prefix}script s=\n{prefix}doubleSided={}\n", part.kind, safe_line(&part.name), part.asset, part.texture, if part.double_sided { 1 } else { 0 }));
        for (axis, value) in ["px", "py", "pz"].iter().zip(part.position) {
            text.push_str(&format!("{prefix}{axis} f={value:.6}\n"));
        }
        for (axis, value) in ["rx", "ry", "rz"].iter().zip(part.rotation) {
            text.push_str(&format!("{prefix}{axis} f={value:.6}\n"));
        }
        for (axis, value) in ["sx", "sy", "sz"].iter().zip(part.scale) {
            text.push_str(&format!("{prefix}{axis} f={value:.6}\n"));
        }
        if part.alpha != 0 {
            text.push_str(&format!("{prefix}alpha={}\n{prefix}opacity f={:.6}\n", part.alpha, part.opacity));
        }
    }
    fs::write(out, text).map_err(|e| e.to_string())
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
mod tests {
    use super::*;

    /// One triangle mesh, a PNG texture, and `nodes` as the node list.
    fn gltf(nodes: &str, roots: &str) -> String {
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":{roots}}}],"nodes":{nodes},
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
            "textures":[{{"source":0}}],"images":[{{"uri":"data:image/png;base64,iVBORw0KGgo="}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA"}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}}]}}"#
        )
    }

    fn imported(project: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(project.join("assets/imported"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn shared_meshes_warnings_and_cleanup() {
        let project = std::env::temp_dir().join(format!("j3_gltf_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(&project).unwrap();
        let scene = project.join("out.j3scene");
        let source = project.join("m.gltf");

        // Three nodes share mesh 0, one of them skinned. No texture coordinates, so the
        // primitive is skipped with a warning and the rest still comes in.
        let nodes = r#"[{"mesh":0},{"mesh":0},{"mesh":0,"skin":0}]"#;
        fs::write(&source, gltf(nodes, "[0,1,2]")).unwrap();
        let warnings = import(&project, &scene, &source, false).unwrap();
        assert!(warnings.contains(&"skinning was ignored".to_string()), "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("TEXCOORD_0")), "{warnings:?}");
        assert_eq!(warnings.len(), 2, "{warnings:?}");

        // Same file with UVs: one mesh file for three nodes.
        let text = gltf(nodes, "[0,1,2]")
            .replace(r#""POSITION":0"#, r#""POSITION":0,"TEXCOORD_0":1"#)
            .replace(r#""type":"VEC3"}]"#, r#""type":"VEC3"},{"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}]"#);
        fs::write(&source, text).unwrap();
        import(&project, &scene, &source, false).unwrap();
        assert_eq!(imported(&project).iter().filter(|n| n.starts_with("m_1_mesh")).count(), 1);
        assert_eq!(fs::read_to_string(&scene).unwrap().matches("m_1_mesh0_prim0").count(), 3);

        // A missing child fails after the image was written; nothing of it is left.
        let before = imported(&project);
        fs::write(&source, gltf(r#"[{"mesh":0,"children":[9]}]"#, "[0]")).unwrap();
        assert!(import(&project, &scene, &source, false).is_err());
        assert_eq!(imported(&project), before);

        // A node reached from two roots must fail before it can duplicate a subtree.
        fs::write(&source, gltf(r#"[{"children":[2]},{"children":[2]},{"mesh":0}]"#, "[0,1]")).unwrap();
        assert_eq!(
            import(&project, &scene, &source, false).unwrap_err(),
            "glTF: node 2 has more than one parent"
        );
        assert_eq!(imported(&project), before);

        // Percent-encoded spaces resolve beside the glTF; a missing image is a warning.
        let external = gltf(r#"[{"mesh":0}]"#, "[0]")
            .replace("data:image/png;base64,iVBORw0KGgo=", "tex%20a.png");
        fs::write(project.join("tex a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        fs::write(project.join("a..b.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        assert_eq!(uri_bytes(&project, "a..b.png").unwrap(), b"\x89PNG\r\n\x1a\n");
        assert!(uri_bytes(&project, "%2e%2e/outside.png").is_err());
        fs::write(&source, &external).unwrap();
        import(&project, &scene, &source, false).unwrap();
        assert!(imported(&project).iter().any(|name| name.ends_with("_image0.png")));
        fs::remove_file(project.join("tex a.png")).unwrap();
        fs::write(&source, external).unwrap();
        let warnings = import(&project, &scene, &source, false).unwrap();
        assert!(warnings.contains(&"some textures could not be read".to_string()));

        let pbr = gltf(r#"[{"mesh":0}]"#, "[0]").replace(r#""pbrMetallicRoughness""#, r#""normalTexture":{"index":0},"pbrMetallicRoughness""#);
        fs::write(&source, pbr).unwrap();
        let warnings = import(&project, &scene, &source, false).unwrap();
        assert!(warnings.contains(&"normal, metal/roughness, and occlusion maps were ignored".to_string()), "{warnings:?}");

        let compressed = gltf(r#"[{"mesh":0}]"#, "[0]")
            .replace("\"scenes\":", "\"extensionsRequired\":[\"KHR_draco_mesh_compression\"],\"scenes\":");
        fs::write(&source, compressed).unwrap();
        assert_eq!(
            import(&project, &scene, &source, false).unwrap_err(),
            "glTF: needs KHR_draco_mesh_compression; re-export without compression"
        );

        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn bake_samples_texture_at_centroid() {
        let project = std::env::temp_dir().join(format!("j3_gltf_bake_{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(&project).unwrap();
        let scene = project.join("out.j3scene");
        let source = project.join("m.gltf");
        fs::write(project.join("pal.png"), include_bytes!("../examples/png_check/pal.png")).unwrap();
        // The position floats read as VEC2 UVs are (0,0), (0,1), (0,0): centroid (0, 1/3),
        // which is pixel (0, 5) of the 19x17 image.
        let text = gltf(r#"[{"mesh":0}]"#, "[0]")
            .replace("data:image/png;base64,iVBORw0KGgo=", "pal.png")
            .replace(r#""POSITION":0"#, r#""POSITION":0,"TEXCOORD_0":1"#)
            .replace(r#""type":"VEC3"}]"#, r#""type":"VEC3"},{"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}]"#);
        fs::write(&source, text).unwrap();
        let warnings = import(&project, &scene, &source, true).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let mesh = fs::read_to_string(project.join("assets/imported/m_mesh0_prim0.j3mesh")).unwrap();
        assert!(mesh.starts_with("# j3mesh 1\n"), "{mesh}");
        let row: Vec<f32> = mesh.lines().nth(2).unwrap().split(' ').map(|v| v.parse().unwrap()).collect();
        let rgba = include_bytes!("../examples/png_check/pal.rgba");
        let at = (5 * 19) * 4;
        for c in 0..3 {
            assert!((row[6 + c] - rgba[at + c] as f32 / 255.0).abs() < 1e-3, "{row:?}");
        }
        let fragment = fs::read_to_string(&scene).unwrap();
        assert!(!fragment.contains("_image0"), "{fragment}");
        let _ = fs::remove_dir_all(&project);
    }
}
