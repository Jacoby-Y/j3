use super::*;

/// Linear 0-1 to sRGB 0-1.
pub(super) fn srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

pub(super) fn arr(value: &Json) -> &[Json] {
    match value {
        Json::Arr(xs) => xs,
        _ => &[],
    }
}

pub(super) fn field<'a>(value: &'a Json, name: &str) -> Result<&'a Json, String> {
    value
        .get(name)
        .ok_or_else(|| format!("glTF: missing `{name}`"))
}

pub(super) fn usize_field(value: &Json, name: &str) -> Result<usize, String> {
    field(value, name)?
        .as_i64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("glTF: invalid `{name}`"))
}

pub(super) fn optional_usize(value: &Json, name: &str) -> Option<usize> {
    value
        .get(name)?
        .as_i64()
        .and_then(|n| usize::try_from(n).ok())
}

pub(super) fn optional_str<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str()
}

pub(super) fn item<'a>(root: &'a Json, table: &str, index: usize) -> Result<&'a Json, String> {
    arr(field(root, table)?)
        .get(index)
        .ok_or_else(|| format!("glTF: {table}[{index}] is missing"))
}

pub(super) fn f32_at(value: &Json, index: usize, default: f32) -> f32 {
    arr(value)
        .get(index)
        .and_then(Json::as_f64)
        .map(|n| n as f32)
        .unwrap_or(default)
}

pub(super) fn vector(value: Option<&Json>, len: usize, default: f32) -> Vec<f32> {
    (0..len)
        .map(|i| value.map(|v| f32_at(v, i, default)).unwrap_or(default))
        .collect()
}

/// Note an unsupported feature that was skipped. Each distinct message is kept once.
pub(super) fn warn(warnings: &mut Vec<String>, message: &str) {
    if !warnings.iter().any(|w| w == message) {
        warnings.push(message.into());
    }
}

pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes.get(offset..offset + 4).ok_or("glTF: truncated GLB")?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}

pub(super) fn read_source(path: &Path) -> Result<(Json, Option<Vec<u8>>), String> {
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

pub(super) fn decode64(encoded: &str) -> Result<Vec<u8>, String> {
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

pub(super) fn uri_bytes(base: &Path, uri: &str) -> Result<Vec<u8>, String> {
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

pub(super) fn buffers(root: &Json, path: &Path, bin: Option<Vec<u8>>) -> Result<Vec<Vec<u8>>, String> {
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

pub(super) struct Accessor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) start: usize,
    pub(super) stride: usize,
    pub(super) count: usize,
    pub(super) component: usize,
    pub(super) width: usize,
}

pub(super) fn accessor<'a>(root: &Json, buffers: &'a [Vec<u8>], index: usize) -> Result<Accessor<'a>, String> {
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
    pub(super) fn scalar(&self, index: usize, lane: usize) -> Result<f32, String> {
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

    pub(super) fn index(&self, index: usize) -> Result<usize, String> {
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
