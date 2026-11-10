use crate::diag::{self, Diagnostic, Level};
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct CheckError {
    pub path: String,
    pub span: Span,
    pub message: String,
    pub notes: Vec<String>,
    pub helps: Vec<String>,
    /// Further independent errors (other function bodies), rendered after this one.
    pub more: Vec<CheckError>,
    /// Errors past the cap that were dropped (rendered as a count).
    pub omitted: usize,
    /// Set by `unknown name/function `x``: a later use of `x` in the same fn is a follow-on.
    pub(crate) unknown: Option<String>,
}

impl CheckError {
    pub fn new(path: impl Into<String>, span: Span, message: impl Into<String>) -> CheckError {
        CheckError {
            path: path.into(),
            span,
            message: message.into(),
            notes: Vec::new(),
            helps: Vec::new(),
            more: Vec::new(),
            omitted: 0,
            unknown: None,
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
        let mut out = self.render_one(sources);
        for e in &self.more {
            out.push('\n');
            out.push_str(&e.render_one(sources));
        }
        let shown = self.more.len() + 1;
        if self.omitted > 0 {
            out.push_str(&format!("\nerror: {shown} errors (and {} more not shown)\n", self.omitted));
        } else if shown > 1 {
            out.push_str(&format!("\nerror: {shown} errors\n"));
        }
        out
    }

    /// This error and its follow-ups, in order.
    pub fn all(&self) -> impl Iterator<Item = &CheckError> {
        std::iter::once(self).chain(self.more.iter())
    }

    /// Applies `f` to this error and every follow-up.
    pub fn map_all(mut self, f: &mut dyn FnMut(CheckError) -> CheckError) -> CheckError {
        let more = std::mem::take(&mut self.more);
        let mut e = f(self);
        e.more = more.into_iter().map(|m| f(m)).collect();
        e
    }

    fn render_one(&self, sources: &[(String, String)]) -> String {
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

/// `no field `y` on `P``, with a near name or the list of fields.
pub(crate) fn no_field(cx: &crate::ir::cx::IrCx, path: &str, span: Span, sid: crate::ir::ids::StructId, name: &str) -> CheckError {
    let def = cx.struct_def(sid);
    let fields: Vec<String> = def.fields.iter().map(|f| cx.name_str(f.name).to_string()).collect();
    let e = CheckError::new(path, span, format!("no field `{name}` on `{}`", cx.name_str(def.name)));
    match suggest(name, &fields) {
        Some(s) => e.help(format!("did you mean `{s}`?")),
        None if fields.is_empty() => e.note(format!("`{}` has no fields", cx.name_str(def.name))),
        None => e.note(format!("`{}` has {}", cx.name_str(def.name), fields.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", "))),
    }
}

pub fn suggest(name: &str, candidates: &[String]) -> Option<String> {
    diag::did_you_mean(name, candidates)
}
