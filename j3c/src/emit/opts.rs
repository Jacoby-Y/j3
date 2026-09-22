use crate::emit::host::CcOpts;
use crate::project::Manifest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    C,
    Llvm,
}

impl Backend {
    pub fn parse(s: &str) -> Result<Backend, String> {
        match s {
            "c" | "C" => Ok(Backend::C),
            "llvm" | "LLVM" | "ll" => Ok(Backend::Llvm),
            other => Err(format!(
                "unknown emit backend `{other}` (want `llvm` or `c`)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::C => "c",
            Backend::Llvm => "llvm",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EmitOpts {
    pub backend: Backend,
    pub cc: CcOpts,
    pub target: String,
    pub cpu: String,
    pub clang: String,
    pub flags: Vec<String>,
    /// Process cwd when running the binary (package root).
    pub cwd: Option<std::path::PathBuf>,
}

impl Default for EmitOpts {
    fn default() -> EmitOpts {
        EmitOpts {
            backend: Backend::Llvm,
            cc: CcOpts {
                debug: true,
                opt: "0".to_string(),
                ..CcOpts::default()
            },
            target: String::new(),
            cpu: String::new(),
            clang: "clang".to_string(),
            flags: Vec::new(),
            cwd: None,
        }
    }
}

fn valid_opt(s: &str) -> bool {
    matches!(s, "0" | "1" | "2" | "3" | "s" | "z" | "S" | "Z")
}

impl EmitOpts {
    pub fn from_manifest(m: &Manifest) -> Result<EmitOpts, String> {
        let backend = Backend::parse(&m.emit.backend)?;
        if !valid_opt(&m.emit.opt) {
            return Err(format!(
                "unknown emit.opt `{}` (want 0, 1, 2, 3, s, or z)",
                m.emit.opt
            ));
        }
        let mut cc = CcOpts::from_manifest(m);
        cc.opt = m.emit.opt.clone();
        cc.debug = m.emit.debug && !m.emit.fast && !m.cc.fast;
        Ok(EmitOpts {
            backend,
            cc,
            target: m.emit.primary().to_string(),
            cpu: m.emit.cpu.clone(),
            clang: if m.emit.clang.is_empty() {
                "clang".to_string()
            } else {
                m.emit.clang.clone()
            },
            flags: m.emit.flags.clone(),
            cwd: Some(m.root.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_llvm_opt0() {
        let o = EmitOpts::default();
        assert_eq!(o.backend, Backend::Llvm);
        assert_eq!(o.cc.opt, "0");
        assert!(o.cc.debug);
    }

    #[test]
    fn backend_parse() {
        assert_eq!(Backend::parse("llvm").unwrap(), Backend::Llvm);
        assert_eq!(Backend::parse("c").unwrap(), Backend::C);
        assert!(Backend::parse("cranelift").is_err());
    }
}
