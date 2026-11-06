//! Pretty-print from the same lexer the compiler uses. Comments stay.

use crate::parse::lex;
use crate::parse::{Token, TokenKind};
use crate::span::FileId;

pub fn format_src(src: &str) -> Result<String, String> {
    let tokens = lex::lex(src, FileId(1)).map_err(|e| e.message.clone())?;
    Ok(reprint(src, &tokens))
}

fn reprint(src: &str, tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut indent: i32 = 0;
    let mut at_line = true;
    let mut prev: Option<TokenKind> = None;
    let mut prev_prefix = false; // prev is `& - * ! ~` in prefix position
    let mut prev_end: usize = 0;

    for tok in tokens {
        if tok.kind == TokenKind::Eof {
            break;
        }
        let start = tok.span.start as usize;
        let gap = if start >= prev_end {
            &src[prev_end..start]
        } else {
            ""
        };
        emit_gap(&mut out, gap, indent, &mut at_line);

        if tok.kind == TokenKind::Doc && !at_line {
            out.push('\n');
            at_line = true;
        }

        if tok.kind == TokenKind::RBrace {
            indent = (indent - 1).max(0);
            if !at_line {
                out.push('\n');
                at_line = true;
            }
        }

        if at_line {
            push_indent(&mut out, indent);
            at_line = false;
        } else if needs_space(prev, tok.kind, prev_prefix) {
            out.push(' ');
        }

        out.push_str(token_src(src, tok));

        match tok.kind {
            TokenKind::Doc => {
                out.push('\n');
                at_line = true;
            }
            TokenKind::LBrace => {
                indent += 1;
                if !next_is(tokens, tok, TokenKind::RBrace) {
                    out.push('\n');
                    at_line = true;
                }
            }
            TokenKind::Semi => {
                if !next_is(tokens, tok, TokenKind::RBrace) && !trailing_comment(src, tok) {
                    out.push('\n');
                    at_line = true;
                }
            }
            TokenKind::RBrace => {
                if !next_is(tokens, tok, TokenKind::Eof)
                    && !next_is(tokens, tok, TokenKind::Else)
                    && !next_is(tokens, tok, TokenKind::Semi)
                {
                    out.push('\n');
                    at_line = true;
                }
            }
            _ => {}
        }

        prev_prefix = match tok.kind {
            TokenKind::Not | TokenKind::Tilde => true,
            TokenKind::Amp | TokenKind::Minus | TokenKind::Star => !prev.is_some_and(ends_operand),
            _ => false,
        };
        prev = Some(tok.kind);
        prev_end = tok.span.end as usize;
    }

    let tail = if prev_end < src.len() {
        &src[prev_end..]
    } else {
        ""
    };
    emit_gap(&mut out, tail, indent, &mut at_line);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn token_src<'a>(src: &'a str, tok: &'a Token) -> &'a str {
    let a = tok.span.start as usize;
    let b = tok.span.end as usize;
    if a <= b && b <= src.len() {
        &src[a..b]
    } else {
        tok.text.as_str()
    }
}

fn next_is(tokens: &[Token], cur: &Token, want: TokenKind) -> bool {
    tokens
        .iter()
        .skip_while(|t| t.span.start <= cur.span.start)
        .find(|t| t.kind != TokenKind::Eof)
        .map(|t| t.kind == want)
        .unwrap_or(false)
}

fn trailing_comment(src: &str, tok: &Token) -> bool {
    let i = tok.span.end as usize;
    let rest = src.get(i..).unwrap_or("");
    let line = rest.split('\n').next().unwrap_or("");
    line.contains("//")
}

fn emit_gap(out: &mut String, gap: &str, indent: i32, at_line: &mut bool) {
    if gap.is_empty() {
        return;
    }
    let mut saw_nl = false;
    let mut blank = false;
    let mut nls = 0;
    for ch in gap.chars() {
        if ch == '\n' {
            nls += 1;
            saw_nl = true;
            if nls >= 2 {
                blank = true;
            }
        }
    }
    if let Some(idx) = gap.find("//") {
        let after = &gap[idx..];
        let comment = after.split('\n').next().unwrap_or(after).trim_end();
        let trailing = !gap[..idx].contains('\n');
        if trailing && !*at_line {
            out.push(' ');
            out.push_str(comment);
            if saw_nl {
                out.push('\n');
                *at_line = true;
            }
        } else {
            if blank && !out.ends_with("\n\n") {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            } else if !*at_line {
                out.push('\n');
            }
            push_indent(out, indent);
            out.push_str(comment);
            out.push('\n');
            *at_line = true;
        }
        if let Some(more) = after.find('\n') {
            emit_gap(out, &after[more + 1..], indent, at_line);
        }
        return;
    }
    if blank && !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
        *at_line = true;
    } else if saw_nl && !*at_line {
        out.push('\n');
        *at_line = true;
    }
}

fn push_indent(out: &mut String, indent: i32) {
    for _ in 0..indent.max(0) {
        out.push_str("    ");
    }
}

/// Token that can end an operand, so a following `& - *` is binary.
fn ends_operand(k: TokenKind) -> bool {
    use TokenKind::*;
    matches!(k, Ident | Int | Float | String | Interpolate | RParen | RBracket | This)
}

fn needs_space(prev: Option<TokenKind>, next: TokenKind, prev_prefix: bool) -> bool {
    let Some(p) = prev else {
        return false;
    };
    // `xs[0]` / `f()[0]` index tight; `= [..]`, `, [..]`, `: []u8` keep the space
    if next == TokenKind::LBracket {
        return !(ends_operand(p) || prev_prefix);
    }
    // `[]u8`, `[4]i32`: a type's element name follows its `]` directly
    if p == TokenKind::RBracket && next == TokenKind::Ident {
        return false;
    }
    if next == TokenKind::LParen {
        return matches!(
            p,
            TokenKind::If | TokenKind::While | TokenKind::For | TokenKind::Match
        );
    }
    if matches!(
        next,
        TokenKind::Doc
            | TokenKind::Comma
            | TokenKind::Semi
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::Colon
            | TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::DotDotEq
            | TokenKind::ColonColon
            | TokenKind::Question
    ) {
        return false;
    }
    if matches!(
        p,
        TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::DotDotEq
            | TokenKind::ColonColon
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::Hash
            | TokenKind::At
            | TokenKind::Not
            | TokenKind::Tilde
    ) {
        return false;
    }
    if matches!(p, TokenKind::Amp | TokenKind::Minus | TokenKind::Star) && prev_prefix {
        return false;
    }
    if p == TokenKind::LBrace && next == TokenKind::RBrace {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_ref_and_array_literal_spacing() {
        let out = format_src("fn main(){let e=[&a,&b];let x=a&b;let y=xs[0];}").expect("fmt");
        assert!(out.contains("let e = [&a, &b];"), "{out}");
        assert!(out.contains("let x = a & b;"), "{out}");
        assert!(out.contains("xs[0]"), "{out}");
    }

    #[test]
    fn minus_and_prefix_spacing_unchanged() {
        let out = format_src("fn main(){let a=b-c;let d=b- -c;f(-x);return -1;}").expect("fmt");
        assert!(out.contains("let a = b - c;"), "{out}");
        assert!(out.contains("let d = b - -c;"), "{out}");
        assert!(out.contains("f(-x);"), "{out}");
        assert!(out.contains("return -1;"), "{out}");
    }

    #[test]
    fn exclusive_range_stays_together() {
        let out = format_src("fn main(){let r=0..4;}").expect("fmt");
        assert!(out.contains("let r = 0..4;"), "{out}");
    }

    #[test]
    fn range_between_names_stays_together() {
        let out = format_src("fn main(){let r=a..b;}").expect("fmt");
        assert!(out.contains("let r = a..b;"), "{out}");
    }

    #[test]
    fn index_range_stays_together() {
        let out = format_src("fn main(){let s=xs[a..b];}").expect("fmt");
        assert!(out.contains("let s = xs[a..b];"), "{out}");
    }

    #[test]
    fn slice_type_stays_together() {
        let out = format_src("fn f(b:[]u8){}").expect("fmt");
        assert!(out.contains("b: []u8"), "{out}");
    }

    #[test]
    fn sized_array_type_stays_together() {
        let out = format_src("fn main(){let v:[4]i32=[0,0,0,0];}").expect("fmt");
        assert!(out.contains("let v: [4]i32 = [0, 0, 0, 0];"), "{out}");
    }

    #[test]
    fn slice_of_struct_type_stays_together() {
        let out = format_src("fn f(vs:[]Vec2){}").expect("fmt");
        assert!(out.contains("vs: []Vec2"), "{out}");
    }

    #[test]
    fn formats_hello_and_keeps_comment() {
        let src = "module hello;\nuse j3.io.printLn;\n// hi\nfn main(){printLn(\"Hello, world!\");}\n";
        let out = format_src(src).expect("fmt");
        assert!(out.contains("module hello;"), "{out}");
        assert!(out.contains("// hi"), "{out}");
        assert!(out.contains("fn main()"), "{out}");
        assert!(out.contains("    printLn("), "{out}");
        let again = format_src(&out).expect("fmt2");
        assert_eq!(out, again);
    }

    #[test]
    fn inclusive_range_stays_together() {
        let src = "module t;\nfn main() {\nfor i in 0..=3 {\n}\n}\n";
        let out = format_src(src).expect("fmt");
        assert!(out.contains("0..=3"), "{out}");
        let again = format_src(&out).expect("fmt2");
        assert_eq!(out, again);
    }

    #[test]
    fn script_blocks_format_and_stay_stable() {
        let src = "props{hinge:int;angle:f32=90.0;}\nshared{count:int=0;}\nlet open=false;\n";
        let out = format_src(src).expect("fmt script");
        assert!(out.contains("props {\n    hinge: int;"), "{out}");
        assert!(out.contains("shared {\n    count: int = 0;"), "{out}");
        assert_eq!(out, format_src(&out).expect("fmt script twice"));
    }

    #[test]
    fn extend_block_formats_like_a_struct_body() {
        let out = format_src("module t;\nextend Box{fn get(this:&Box):int{return this.v;}}\n").expect("fmt");
        assert_eq!(out, "module t;\nextend Box {\n    fn get(this: &Box): int {\n        return this.v;\n    }\n}\n");
    }
}
