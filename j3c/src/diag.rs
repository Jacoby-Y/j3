//! rustc-shaped diagnostics. No extra crates. Color is opt-in (`J3C_COLOR=1`).

use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub path: String,
    pub span: Span,
    pub message: String,
    pub notes: Vec<String>,
    pub helps: Vec<String>,
}

impl Diagnostic {
    pub fn error(path: impl Into<String>, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            level: Level::Error,
            path: path.into(),
            span,
            message: message.into(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }

    pub fn warning(path: impl Into<String>, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { level: Level::Warning, ..Diagnostic::error(path, span, message) }
    }

    pub fn note(mut self, msg: impl Into<String>) -> Diagnostic {
        self.notes.push(msg.into());
        self
    }

    pub fn help(mut self, msg: impl Into<String>) -> Diagnostic {
        self.helps.push(msg.into());
        self
    }

    pub fn render(&self, sources: &[(String, String)]) -> String {
        let src = sources
            .iter()
            .find(|(p, _)| p == &self.path)
            .map(|(_, s)| s.as_str());
        render(self.level, &self.path, self.span, &self.message, src, &self.notes, &self.helps)
    }
}

pub fn loc(src: &str, offset: u32) -> (usize, usize) {
    let n = (offset as usize).min(src.len());
    let line = src[..n].bytes().filter(|b| *b == b'\n').count() + 1;
    let col = n - src[..n].rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
    (line, col)
}

pub fn line_text(src: &str, line: usize) -> &str {
    src.lines().nth(line.saturating_sub(1)).unwrap_or("")
}

fn render(
    level: Level,
    path: &str,
    span: Span,
    message: &str,
    src: Option<&str>,
    notes: &[String],
    helps: &[String],
) -> String {
    let kind = match level {
        Level::Error => "error",
        Level::Warning => "warning",
    };
    let color = std::env::var_os("J3C_COLOR").is_some();
    let (head, reset) = if color {
        match level {
            Level::Error => ("\x1b[1;31m", "\x1b[0m"),
            Level::Warning => ("\x1b[1;33m", "\x1b[0m"),
        }
    } else {
        ("", "")
    };
    let mut out = format!("{head}{kind}{reset}: {message}\n");
    if let Some(src) = src {
        if !span.is_dummy() {
            let (line, col) = loc(src, span.start);
            let (end_line, end_col) = loc(src, span.end.max(span.start + 1));
            out.push_str(&format!("  --> {path}:{line}:{col}\n"));
            out.push_str("   |\n");
            let text = line_text(src, line);
            out.push_str(&format!("{line:>3} | {text}\n"));
            let mark_len = if end_line == line {
                end_col.saturating_sub(col).max(1)
            } else {
                text.len().saturating_sub(col.saturating_sub(1)).max(1)
            };
            let pad = " ".repeat(col.saturating_sub(1));
            let marks = "^".repeat(mark_len.min(64));
            out.push_str(&format!("   | {pad}{marks}\n"));
        } else {
            out.push_str(&format!("  --> {path}\n"));
        }
    } else if !path.is_empty() {
        out.push_str(&format!("  --> {path}\n"));
    }
    if !notes.is_empty() || !helps.is_empty() {
        out.push_str("   |\n");
    }
    for n in notes {
        out.push_str(&format!("   = note: {n}\n"));
    }
    for h in helps {
        out.push_str(&format!("   = help: {h}\n"));
    }
    out
}

/// Closest identifier. Distance 1, or 2 when the name is longer than 4.
pub fn did_you_mean(name: &str, candidates: &[String]) -> Option<String> {
    if name.is_empty() || candidates.is_empty() {
        return None;
    }
    let max = if name.len() <= 4 { 1 } else { 2 };
    // On a tie, a candidate sharing the name's first letters wins (`y` -> `yy`, not `x`).
    let mut best: Option<((usize, bool), &str)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let d = edit_distance(name, c);
        if d == 0 || d > max {
            continue;
        }
        let key = (d, !(c.starts_with(name) || name.starts_with(c.as_str())));
        if best.map(|(bk, _)| key < bk).unwrap_or(true) {
            best = Some((key, c.as_str()));
        }
    }
    best.map(|(_, s)| s.to_string())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    // Levenshtein plus adjacent swaps (`grwo` -> `grow` is one edit)
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
            if i > 0 && j > 0 && *ca == b[j - 1] && a[i - 1] == *cb {
                cur[j + 1] = cur[j + 1].min(prev2[j - 1] + 1);
            }
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::FileId;

    #[test]
    fn snippet_and_help() {
        let src = "fn main() {\n    let x = y;\n}\n";
        let d = Diagnostic::error("t.j3", Span { file: FileId(1), start: 21, end: 22 }, "unknown name `y`")
            .help("did you mean `x`?");
        let s = d.render(&[("t.j3".into(), src.into())]);
        assert!(s.contains("error: unknown name `y`"), "{s}");
        assert!(s.contains("--> t.j3:2:"), "{s}");
        assert!(s.contains("let x = y;"), "{s}");
        assert!(s.contains("help: did you mean `x`?"), "{s}");
    }

    #[test]
    fn suggest_typo() {
        let names = ["start", "step", "boot"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        assert_eq!(did_you_mean("statr", &names).as_deref(), Some("start"));
        assert_eq!(did_you_mean("stp", &names).as_deref(), Some("step"));
        assert_eq!(did_you_mean("zzzz", &names), None);
        assert_eq!(did_you_mean("stpe", &names).as_deref(), Some("step"));
    }
}
