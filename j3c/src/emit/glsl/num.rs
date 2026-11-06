//! Number literals.

/// A float literal GLSL reads as `float` (always has a `.` or exponent).
pub fn float(v: f64) -> String {
    let f = v as f32;
    let s = format!("{f:?}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// Whether `s` is a plain decimal integer literal (so `.0` or `u` can be appended to it).
pub fn is_int_text(s: &str) -> bool {
    let t = s.strip_prefix('-').unwrap_or(s);
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// `(a + b)` as `a + b`: the outer parentheses go when they wrap the whole text (statement positions need none).
pub fn bare(s: &str) -> &str {
    if !(s.starts_with('(') && s.ends_with(')')) {
        return s;
    }
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i != s.len() - 1 {
                    return s;
                }
            }
            _ => {}
        }
    }
    &s[1..s.len() - 1]
}
