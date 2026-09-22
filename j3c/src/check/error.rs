use crate::diag::{self, Diagnostic, Level};
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct CheckError {
    pub path: String,
    pub span: Span,
    pub message: String,
    pub notes: Vec<String>,
    pub helps: Vec<String>,
}

impl CheckError {
    pub fn new(path: impl Into<String>, span: Span, message: impl Into<String>) -> CheckError {
        CheckError {
            path: path.into(),
            span,
            message: message.into(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }

    pub fn note(mut self, msg: impl Into<String>) -> CheckError {
        self.notes.push(msg.into());
        self
    }

    pub fn help(mut self, msg: impl Into<String>) -> CheckError {
        self.helps.push(msg.into());
        self
    }

    pub fn display(&self, src: &str) -> String {
        self.render(&[(self.path.clone(), src.to_string())])
    }

    pub fn render(&self, sources: &[(String, String)]) -> String {
        Diagnostic {
            level: Level::Error,
            path: self.path.clone(),
            span: self.span,
            message: self.message.clone(),
            notes: self.notes.clone(),
            helps: self.helps.clone(),
        }
        .render(sources)
    }
}

pub fn suggest(name: &str, candidates: &[String]) -> Option<String> {
    diag::did_you_mean(name, candidates)
}
