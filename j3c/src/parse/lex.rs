use crate::parse::error::ParseError;
use crate::parse::token::{keyword, Token, TokenKind};
use crate::span::{FileId, Span};

pub fn lex(src: &str, file: FileId) -> Result<Vec<Token>, ParseError> {
    let mut l = Lexer {
        src,
        file,
        i: 0,
        tokens: Vec::new(),
    };
    l.run()?;
    Ok(l.tokens)
}

struct Lexer<'a> {
    src: &'a str,
    file: FileId,
    i: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) -> Result<(), ParseError> {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
                continue;
            }
            if c == '/' && self.peek_at(1) == Some('/') {
                if self.peek_at(2) == Some('/') && self.peek_at(3) != Some('/') {
                    self.doc_comment();
                } else {
                    self.line_comment();
                }
                continue;
            }
            let start = self.i;
            match c {
                '(' => self.one(TokenKind::LParen, start),
                ')' => self.one(TokenKind::RParen, start),
                '{' => self.one(TokenKind::LBrace, start),
                '}' => self.one(TokenKind::RBrace, start),
                '[' => self.one(TokenKind::LBracket, start),
                ']' => self.one(TokenKind::RBracket, start),
                ',' => self.one(TokenKind::Comma, start),
                ';' => self.one(TokenKind::Semi, start),
                ':' => {
                    self.bump();
                    if self.peek() == Some(':') {
                        self.bump();
                        self.push(TokenKind::ColonColon, start, self.i, "");
                    } else {
                        self.push(TokenKind::Colon, start, self.i, "");
                    }
                }
                '.' => {
                    self.bump();
                    if self.peek() == Some('.') {
                        self.bump();
                        if self.peek() == Some('=') {
                            self.bump();
                            self.push(TokenKind::DotDotEq, start, self.i, "");
                        } else {
                            self.push(TokenKind::DotDot, start, self.i, "");
                        }
                    } else {
                        self.push(TokenKind::Dot, start, self.i, "");
                    }
                }
                '@' => self.one(TokenKind::At, start),
                '#' => self.one(TokenKind::Hash, start),
                '?' => self.one(TokenKind::Question, start),
                '&' => {
                    self.bump();
                    if self.peek() == Some('&') {
                        self.bump();
                        self.push(TokenKind::AndAnd, start, self.i, "");
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::AmpEq, start, self.i, "");
                    } else {
                        self.push(TokenKind::Amp, start, self.i, "");
                    }
                }
                '^' => self.assign_op(start, TokenKind::Caret, TokenKind::CaretEq),
                '~' => self.one(TokenKind::Tilde, start),
                '|' => {
                    self.bump();
                    if self.peek() == Some('|') {
                        self.bump();
                        self.push(TokenKind::OrOr, start, self.i, "");
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::PipeEq, start, self.i, "");
                    } else {
                        self.push(TokenKind::Pipe, start, self.i, "");
                    }
                }
                '!' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Ne, start, self.i, "");
                    } else {
                        self.push(TokenKind::Not, start, self.i, "");
                    }
                }
                '<' if self.peek_at(1) == Some('<') && self.peek_at(2) == Some('=') => {
                    self.i += 3;
                    self.push(TokenKind::ShlEq, start, self.i, "");
                }
                '>' if self.peek_at(1) == Some('>') && self.peek_at(2) == Some('=') => {
                    self.i += 3;
                    self.push(TokenKind::ShrEq, start, self.i, "");
                }
                '<' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Le, start, self.i, "");
                    } else {
                        self.push(TokenKind::Lt, start, self.i, "");
                    }
                }
                '>' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Ge, start, self.i, "");
                    } else {
                        self.push(TokenKind::Gt, start, self.i, "");
                    }
                }
                '=' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::EqEq, start, self.i, "");
                    } else {
                        self.push(TokenKind::Eq, start, self.i, "");
                    }
                }
                '+' => self.assign_op(start, TokenKind::Plus, TokenKind::PlusEq),
                '-' => self.assign_op(start, TokenKind::Minus, TokenKind::MinusEq),
                '*' => self.assign_op(start, TokenKind::Star, TokenKind::StarEq),
                '/' => self.assign_op(start, TokenKind::Slash, TokenKind::SlashEq),
                '%' => self.assign_op(start, TokenKind::Percent, TokenKind::PercentEq),
                '"' => self.string(start)?,
                '`' => self.interpolate(start)?,
                '\'' => self.quote(start)?,
                c if is_ident_start(c) => self.ident(start),
                c if c.is_ascii_digit() => self.number(start)?,
                _ => {
                    return Err(ParseError::new(
                        self.span(start, self.next_end()),
                        format!("unexpected character {:?}", c),
                    ));
                }
            }
        }
        let end = self.src.len() as u32;
        self.tokens.push(Token::eof(Span {
            file: self.file,
            start: end,
            end,
        }));
        Ok(())
    }

    fn one(&mut self, kind: TokenKind, start: usize) {
        self.bump();
        self.push(kind, start, self.i, "");
    }

    fn assign_op(&mut self, start: usize, alone: TokenKind, with_eq: TokenKind) {
        self.bump();
        if self.peek() == Some('=') {
            self.bump();
            self.push(with_eq, start, self.i, "");
        } else {
            self.push(alone, start, self.i, "");
        }
    }

    fn interpolate(&mut self, start: usize) -> Result<(), ParseError> {
        self.bump();
        let inner_start = self.i;
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => {
                    return Err(ParseError::new(
                        self.span(start, self.i),
                        "unterminated interpolate",
                    ));
                }
            };
            if c == '`' {
                break;
            }
            if c == '\n' {
                return Err(ParseError::new(
                    self.span(start, self.i),
                    "newline in interpolate",
                ));
            }
            self.bump();
        }
        let inner = self.src[inner_start..self.i].to_string();
        self.bump();
        self.tokens.push(Token {
            kind: TokenKind::Interpolate,
            span: self.span(start, self.i),
            text: inner,
        });
        Ok(())
    }

    fn ident(&mut self, start: usize) {
        self.bump();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.bump();
            } else {
                break;
            }
        }
        let text = &self.src[start..self.i];
        let kind = match keyword(text) {
            Some(k) => k,
            None => TokenKind::Ident,
        };
        self.push(kind, start, self.i, text);
    }

    fn number(&mut self, start: usize) -> Result<(), ParseError> {
        if self.peek() == Some('0') && self.peek_at(1) == Some('b') {
            self.bump();
            self.bump();
            if !matches!(self.peek(), Some('0' | '1')) {
                return Err(ParseError::new(self.span(start, self.i), "invalid binary literal"));
            }
            while matches!(self.peek(), Some('0' | '1')) {
                self.bump();
            }
            let text = &self.src[start..self.i];
            self.push(TokenKind::Int, start, self.i, text);
            return Ok(());
        }
        let hex = self.peek() == Some('0') && self.peek_at(1) == Some('x');
        if hex {
            self.bump();
            self.bump();
            if !self.peek().map(|c| c.is_ascii_hexdigit()).unwrap_or(false) {
                return Err(ParseError::new(
                    self.span(start, self.i),
                    "invalid hex literal",
                ));
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    self.bump();
                } else {
                    break;
                }
            }
        } else {
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.bump();
                } else {
                    break;
                }
            }
            // `t.0.1` is two field accesses, not `t.` then the float `0.1`.
            let after_dot = self.tokens.last().is_some_and(|t| t.kind == TokenKind::Dot);
            if !after_dot
                && self.peek() == Some('.')
                && self.peek_at(1).map(|c| c.is_ascii_digit()).unwrap_or(false)
            {
                self.bump();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.exponent(start)?;
                let text = &self.src[start..self.i];
                self.push(TokenKind::Float, start, self.i, text);
                return Ok(());
            }
            if !after_dot && self.exponent(start)? {
                let text = &self.src[start..self.i];
                self.push(TokenKind::Float, start, self.i, text);
                return Ok(());
            }
        }
        let text = &self.src[start..self.i];
        self.push(TokenKind::Int, start, self.i, text);
        Ok(())
    }

    /// `e9`, `E+10`, `e-3` after a number's digits: true when one was read. An `e` with no
    /// digit after it (and no sign) is left alone, so `1.e` stays a field access.
    fn exponent(&mut self, start: usize) -> Result<bool, ParseError> {
        if !matches!(self.peek(), Some('e' | 'E')) {
            return Ok(false);
        }
        let sign = matches!(self.peek_at(1), Some('+' | '-'));
        let digit_at = if sign { 2 } else { 1 };
        if !self.peek_at(digit_at).is_some_and(|c| c.is_ascii_digit()) {
            if sign {
                let end = self.i + digit_at;
                return Err(ParseError::new(self.span(start, end), "malformed float exponent: expected digits after the sign"));
            }
            return Ok(false);
        }
        self.i += digit_at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        Ok(true)
    }

    fn string(&mut self, start: usize) -> Result<(), ParseError> {
        self.bump();
        let mut bytes = Vec::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => {
                    return Err(ParseError::new(
                        self.span(start, self.i),
                        "unterminated string",
                    ));
                }
            };
            if c == '"' {
                self.bump();
                break;
            }
            if c == '\\' {
                self.bump();
                let e = match self.peek() {
                    Some(e) => e,
                    None => {
                        return Err(ParseError::new(
                            self.span(start, self.i),
                            "unterminated string escape",
                        ));
                    }
                };
                self.bump();
                match e {
                    '\\' => bytes.push(b'\\'),
                    '"' => bytes.push(b'"'),
                    'n' => bytes.push(b'\n'),
                    'r' => bytes.push(b'\r'),
                    't' => bytes.push(b'\t'),
                    '0' => bytes.push(0),
                    _ => {
                        return Err(ParseError::new(
                            self.span(self.i - 2, self.i),
                            format!("unknown string escape \\{}", e),
                        ));
                    }
                }
                continue;
            }
            if c == '\n' {
                return Err(ParseError::new(
                    self.span(start, self.i),
                    "newline in string",
                ));
            }
            let ch_start = self.i;
            self.bump();
            bytes.extend(self.src[ch_start..self.i].as_bytes());
        }
        let mut tok = Token {
            kind: TokenKind::String,
            span: self.span(start, self.i),
            text: String::new(),
        };
        match String::from_utf8(bytes) {
            Ok(s) => tok.text = s,
            Err(_) => {
                return Err(ParseError::new(
                    self.span(start, self.i),
                    "string is not UTF-8",
                ));
            }
        }
        self.tokens.push(tok);
        Ok(())
    }

    /// `'a'`, `'\n'`, `'\u{1F600}'` (a `Char` token whose text is the code point in decimal),
    /// else `'rows` (a `Label`). A char is the closing quote right after one char or one escape.
    fn quote(&mut self, start: usize) -> Result<(), ParseError> {
        match char_lit(&self.src[start..]) {
            Some(Ok((v, len))) => {
                self.i = start + len;
                self.push(TokenKind::Char, start, self.i, &v.to_string());
                return Ok(());
            }
            Some(Err((msg, len))) => return Err(ParseError::new(self.span(start, start + len), msg)),
            None => {}
        }
        self.bump();
        if !self.peek().is_some_and(is_ident_start) {
            return Err(ParseError::new(self.span(start, self.i), "unexpected character '\\''"));
        }
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        if self.peek() == Some('\'') {
            return Err(ParseError::new(
                self.span(start, self.i + 1),
                "a char literal holds one character (a string is `\"..\"`)",
            ));
        }
        let text = &self.src[start..self.i];
        self.push(TokenKind::Label, start, self.i, text);
        Ok(())
    }

    fn doc_comment(&mut self) {
        let start = self.i;
        self.bump();
        self.bump();
        self.bump();
        if self.peek() == Some(' ') {
            self.bump();
        }
        let text_start = self.i;
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
        let text = self.src[text_start..self.i].trim_end().to_string();
        self.push(TokenKind::Doc, start, self.i, &text);
    }

    fn line_comment(&mut self) {
        while let Some(c) = self.peek() {
            self.bump();
            if c == '\n' {
                break;
            }
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize, text: &str) {
        self.tokens.push(Token {
            kind,
            span: self.span(start, end),
            text: text.to_string(),
        });
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span {
            file: self.file,
            start: start as u32,
            end: end as u32,
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.i..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src[self.i..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    fn next_end(&self) -> usize {
        match self.peek() {
            Some(c) => self.i + c.len_utf8(),
            None => self.i,
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The char literal at the start of `s` (which starts with `'`): `Some(Ok((code point, byte len)))`,
/// `Some(Err((message, byte len)))` for a bad one, `None` when it is not one (a `'label`).
/// Shared with the LSP's text scanners so a `'"'` does not open a string there.
pub fn char_lit(s: &str) -> Option<Result<(u32, usize), (String, usize)>> {
    let c = s.get(1..)?.chars().next()?;
    if c == '\\' {
        let (v, n) = match char_escape(&s[2..]) {
            Ok(x) => x,
            Err(m) => return Some(Err((m, (2 + s[2..].chars().next().map_or(0, char::len_utf8)).min(s.len())))),
        };
        let end = 2 + n;
        if s[end..].starts_with('\'') {
            return Some(Ok((v, end + 1)));
        }
        return Some(Err(("a char literal holds one character (a string is `\"..\"`)".to_string(), close_at(s, end))));
    }
    if c == '\'' {
        return Some(Err(("empty char literal".to_string(), 2)));
    }
    if c == '\n' {
        return None;
    }
    let end = 1 + c.len_utf8();
    if s[end..].starts_with('\'') {
        return Some(Ok((c as u32, end + 1)));
    }
    if is_ident_start(c) {
        return None;
    }
    if s[end..].split('\n').next().is_some_and(|line| line.contains('\'')) {
        return Some(Err(("a char literal holds one character (a string is `\"..\"`)".to_string(), close_at(s, end))));
    }
    None
}

/// Byte length up to and including the next `'` on this line, else up to `from`.
fn close_at(s: &str, from: usize) -> usize {
    let line = s[from..].split('\n').next().unwrap_or("");
    match line.find('\'') {
        Some(k) => from + k + 1,
        None => from,
    }
}

/// The escape after a `\` in a char literal: its code point and byte length.
fn char_escape(s: &str) -> Result<(u32, usize), String> {
    let c = s.chars().next().ok_or("unterminated char literal")?;
    let simple = match c {
        'n' => Some(10),
        'r' => Some(13),
        't' => Some(9),
        '0' => Some(0),
        '\\' => Some(92),
        '\'' => Some(39),
        '"' => Some(34),
        _ => None,
    };
    if let Some(v) = simple {
        return Ok((v, 1));
    }
    if c == 'x' {
        let hex = s.get(1..3).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
        let v = hex.and_then(|h| u32::from_str_radix(h, 16).ok()).ok_or("`\\x` takes two hex digits")?;
        if v > 0x7f {
            return Err("`\\x` is ASCII (up to `\\x7f`); write `\\u{..}` for a code point".to_string());
        }
        return Ok((v, 3));
    }
    if c == 'u' {
        let body = s.strip_prefix("u{").ok_or("`\\u` takes `{hex}`")?;
        let close = body.find('}').filter(|&k| (1..=6).contains(&k)).ok_or("`\\u{..}` takes 1 to 6 hex digits")?;
        let v = u32::from_str_radix(&body[..close], 16).map_err(|_| "`\\u{..}` takes hex digits")?;
        if char::from_u32(v).is_none() {
            return Err(format!("`\\u{{{}}}` is not a Unicode scalar value", &body[..close]));
        }
        return Ok((v, 3 + close));
    }
    Err(format!("unknown char escape \\{c}"))
}
