//! Folds `extend T { .. }` blocks into `T`'s struct once a module's files are combined, so
//! everything after parsing sees ordinary struct methods. Spans keep the file they were written in.

use super::{ExtendItem, Item};
use crate::span::Span;

pub struct ExtendErr {
    pub span: Span,
    pub message: String,
    pub help: String,
}

/// `module`: the module being merged (for messages). `elsewhere(name)`: the module that declares
/// `name` when it is not this one.
pub fn merge(items: &mut Vec<Item>, module: &str, elsewhere: &dyn Fn(&str) -> Option<String>) -> Result<(), ExtendErr> {
    if !items.iter().any(|i| matches!(i, Item::Extend(_))) {
        return Ok(());
    }
    let mut blocks: Vec<ExtendItem> = Vec::new();
    for it in std::mem::take(items) {
        match it {
            Item::Extend(e) => blocks.push(e),
            other => items.push(other),
        }
    }
    for e in blocks {
        let name = &e.name.name;
        let at = e.name.span;
        let Some(s) = items.iter_mut().find_map(|i| match i {
            Item::Struct(s) if s.name.name == *name => Some(s),
            _ => None,
        }) else {
            return Err(if items.iter().any(|i| matches!(i, Item::Enum(d) if d.name.name == *name)) {
                ExtendErr {
                    span: at,
                    message: format!("cannot `extend` the enum `{name}`; only structs take methods"),
                    help: format!("write a free fn `fn m(this: {name})`; it calls as `x.m()`"),
                }
            } else if let Some(m) = elsewhere(name) {
                ExtendErr {
                    span: at,
                    message: format!("`{name}` is declared in module {m}, not in {module}"),
                    help: format!("methods of `{name}` go in module {m}: its file or a part file"),
                }
            } else {
                ExtendErr {
                    span: at,
                    message: format!("`extend {name}`: no struct `{name}` in module {module}"),
                    help: format!("declare `struct {name}` in module {module} (its file or a part file), or fix the name"),
                }
            });
        };
        let want: Vec<&str> = s.type_params.iter().map(|p| p.name.as_str()).collect();
        let got: Vec<&str> = e.type_params.iter().map(|p| p.name.as_str()).collect();
        if want != got {
            let fmt = |v: &[&str]| if v.is_empty() { name.clone() } else { format!("{name}<{}>", v.join(", ")) };
            return Err(ExtendErr {
                span: at,
                message: format!("`extend {}` does not match `struct {}`", fmt(&got), fmt(&want)),
                help: format!("write `extend {}`; the parameters are named as in the struct", fmt(&want)),
            });
        }
        for m in e.methods {
            if s.methods.iter().any(|o| o.name.name == m.name.name) {
                return Err(ExtendErr {
                    span: m.name.span,
                    message: format!("`{name}` already has a method `{}`", m.name.name),
                    help: "a method is written once, in the struct body or in one `extend` block; rename one".into(),
                });
            }
            s.methods.push(m);
        }
    }
    Ok(())
}
