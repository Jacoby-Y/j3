use crate::span::Span;

#[derive(Clone, Debug)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl ParseError {
    pub fn new(span: Span, message: impl Into<String>) -> ParseError {
        ParseError {
            span,
            message: message.into(),
        }
    }
}
