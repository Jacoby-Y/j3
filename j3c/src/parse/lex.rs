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
                    } else {
                        self.push(TokenKind::Amp, start, self.i, "");
                    }
                }
                '|' => {
                    self.bump();
                    if self.peek() == Some('|') {
                        self.bump();
                        self.push(TokenKind::OrOr, start, self.i, "");
                    } else {
                        return Err(ParseError::new(
                            self.span(start, self.i),
                            "bitwise `|` not in this subset",
                        ));
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
                '"' => self.string(start)?,
                '`' => self.interpolate(start)?,
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
            if self.peek() == Some('.')
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
                let text = &self.src[start..self.i];
                self.push(TokenKind::Float, start, self.i, text);
                return Ok(());
            }
        }
        let text = &self.src[start..self.i];
        self.push(TokenKind::Int, start, self.i, text);
        Ok(())
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
