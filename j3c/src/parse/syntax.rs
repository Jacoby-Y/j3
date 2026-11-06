//! Names the lexer/parser already know, plus a TextMate dump for editors.
//!
//! `j3c dump-syntax` prints [`tm_grammar`]. Do not hand-edit the shipped
//! `editors/vscode/syntaxes/j3.tmLanguage.json` — regenerate it.
//!
//! Scopes follow Dark+ (functions yellow, types teal, variables light-blue,
//! constants cyan, control purple). Keywords still come from [`KEYWORDS`].

use crate::parse::token::{TokenKind, BOOL_LITS, KEYWORDS};

mod rules;
mod rules_tokens;
mod tm;
mod ui;

use rules::*;
use rules_tokens::*;
pub use tm::*;
pub use ui::*;

pub const ATTR_DEBUG: &str = "Debug";
pub const ATTR_SINGLETON: &str = "singleton";
pub const ATTR_TRY_CATCH: &str = "TryCatch";
pub const ATTR_THREAD: &str = "thread";
pub const ATTR_IMPL: &str = "impl";
pub const ATTRIBUTES: &[&str] = &[ATTR_DEBUG, ATTR_SINGLETON, ATTR_TRY_CATCH, ATTR_THREAD, ATTR_IMPL, "shader", "vertex", "varying", "uniform", "flat"];
/// Contextual item word: `trait Name { .. }`, only when an identifier follows.
pub const ITEM_TRAIT: &str = "trait";
/// Contextual item word: `extend Type { fn .. }`, only when an identifier follows.
pub const ITEM_EXTEND: &str = "extend";

pub const INTRINSIC_SYSCALL: &str = "syscall";
pub const INTRINSIC_SIZEOF: &str = "sizeof";
pub const INTRINSIC_UI_CHOICE: &str = "uiChoice";
pub const INTRINSIC_SLICE: &str = "slice";
pub const INTRINSIC_ATOMICS: &[&str] = &[
    "atomicLoad",
    "atomicStore",
    "atomicAdd",
    "atomicSub",
    "atomicExchange",
    "atomicCas",
];
/// Stdlib-only: pull a closure apart into a trampoline's `ptr` pair, or rebuild one (`speculative/lang/closures.j3`).
pub const INTRINSIC_CLOSURES: &[&str] = &["closureFn", "closureCtx", "closureFrom"];
/// `@vmin(a, b)` and friends on `vec4f` / `vec4i`; parsed as calls of the name `@vmin`, lowered in `check/lower/simd.rs`.
pub const INTRINSIC_SIMD: &[&str] = &[
    "vmin", "vmax", "vsqrt", "vabs", "vsum", "vshuffle", "vfloat", "vint", "vload", "vstore",
];
/// Memory-order words, the last argument of `@atomic*(..)` (`ir/op.rs` `AtomicOrder::name`).
pub const ATOMIC_ORDERS: &[&str] = &["relaxed", "acquire", "release", "acqrel", "seqcst"];
/// `@shader(vs, fs)` and the GLSL builtins shader fns call (`check/gpu/builtins.rs` has the rules); parsed as calls of `@name`.
pub const INTRINSIC_GPU: &[&str] = &["shader", "discard", "fragCoord", "vertexId", "mix", "clamp", "dot", "cross", "length", "distance", "normalize", "min", "max", "abs", "sign", "floor", "ceil", "fract", "pow", "exp", "log", "sqrt", "sin", "cos", "tan", "atan", "step", "smoothstep", "mod", "fwidth", "dFdx", "dFdy", "texture", "textureLod", "texelFetch", "textureSize", "reflect", "refract", "transpose", "inverse"];
/// Contextual type word: `closure(A): R`, only when followed by `(`.
pub const TYPE_CLOSURE: &str = "closure";
pub const INTRINSIC_J3UI: &str = "j3ui";
pub const INTRINSIC_SCRIPT: &str = "script";
pub const SCRIPT_PROPS: &str = "props";
pub const SCRIPT_SHARED: &str = "shared";
pub const INTRINSICS: &[&str] = &[
    INTRINSIC_SYSCALL,
    INTRINSIC_SIZEOF,
    INTRINSIC_UI_CHOICE,
    INTRINSIC_SLICE,
    INTRINSIC_J3UI,
    INTRINSIC_SCRIPT,
    "atomicLoad",
    "atomicStore",
    "atomicAdd",
    "atomicSub",
    "atomicExchange",
    "atomicCas",
    "closureFn",
    "closureCtx",
    "closureFrom",
    "vmin",
    "vmax",
    "vsqrt",
    "vabs",
    "vsum",
    "vshuffle",
    "vfloat",
    "vint",
    "vload",
    "vstore",
    "shader", "discard", "fragCoord", "vertexId", "mix", "clamp", "dot", "cross", "length", "distance", "normalize", "min", "max", "abs", "sign", "floor", "ceil", "fract", "pow", "exp", "log", "sqrt", "sin", "cos", "tan", "atan", "step", "smoothstep", "mod", "fwidth", "dFdx", "dFdy", "texture", "textureLod", "texelFetch", "textureSize", "reflect", "refract", "transpose", "inverse",
];

/// `@j3ui` widgets whose body is the call, then `end`.
pub const J3UI_END: &[&str] = &["panel", "row", "scroll", "card"];
/// `@j3ui` widgets whose body is the call, then `popId`.
pub const J3UI_POP: &[&str] = &["pushId"];
/// `@j3ui` widgets whose body runs when the call returns true.
pub const J3UI_IF: &[&str] = &["button"];
/// `@j3ui` widgets that are one call. A `{ }` on these is an error.
pub const J3UI_CALL: &[&str] = &[
    "label",
    "labelBold",
    "gap",
    "buttonOff",
    "bar",
    "image",
    "slider",
    "range",
    "toggle",
    "check",
    "stepper",
    "chip",
    "rule",
    "wash",
    "drop",
    "field",
    "drag",
    "worldLabel",
    "worldLabel3",
    "popId",
    "end",
];

pub fn keyword_names() -> impl Iterator<Item = &'static str> {
    KEYWORDS.iter().map(|(name, _)| *name)
}

pub fn is_j3ui_widget(name: &str) -> bool {
    J3UI_END.contains(&name)
        || J3UI_POP.contains(&name)
        || J3UI_IF.contains(&name)
        || J3UI_CALL.contains(&name)
}

/// Builtin type spellings. Checker must accept every name.
pub const BUILTIN_TYPES: &[&str] = &[
    "void", "bool", "int", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f16", "f32", "f64",
    "float", "ptr", "cstr", "string", "vec4f", "vec4i",
];

const OPERATOR_KINDS: &[TokenKind] = &[
    TokenKind::ColonColon,
    TokenKind::DotDot,
    TokenKind::DotDotEq,
    TokenKind::EqEq,
    TokenKind::Ne,
    TokenKind::Le,
    TokenKind::Ge,
    TokenKind::AndAnd,
    TokenKind::OrOr,
    TokenKind::PlusEq,
    TokenKind::MinusEq,
    TokenKind::StarEq,
    TokenKind::SlashEq,
    TokenKind::PercentEq,
    TokenKind::AmpEq,
    TokenKind::PipeEq,
    TokenKind::CaretEq,
    TokenKind::Pipe,
    TokenKind::Caret,
    TokenKind::Tilde,
    TokenKind::Dot,
    TokenKind::Question,
    TokenKind::Amp,
    TokenKind::Eq,
    TokenKind::Lt,
    TokenKind::Gt,
    TokenKind::Not,
    TokenKind::Plus,
    TokenKind::Minus,
    TokenKind::Star,
    TokenKind::Slash,
    TokenKind::Percent,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::cx::IrCx;

    #[test]
    fn keywords_have_highlight_scopes() {
        for &(_, k) in KEYWORDS {
            assert!(k.highlight_scope().is_some(), "{k:?}");
        }
    }

    #[test]
    fn grammar_contains_compiler_names() {
        let g = tm_grammar();
        for &(name, _) in KEYWORDS {
            assert!(g.contains(name), "missing keyword {name}");
        }
        for name in BOOL_LITS {
            assert!(g.contains(name), "missing bool lit {name}");
        }
        for name in ATTRIBUTES {
            assert!(g.contains(name), "missing attribute {name}");
        }
        for name in INTRINSICS {
            assert!(g.contains(name), "missing intrinsic {name}");
        }
        for name in BUILTIN_TYPES {
            assert!(g.contains(name), "missing type {name}");
        }
        assert!(g.contains(TYPE_CLOSURE), "missing closure type word");
        assert!(g.contains(ITEM_TRAIT), "missing trait item word");
        assert!(g.contains(ITEM_EXTEND), "missing extend item word");
        for k in OPERATOR_KINDS {
            let src = k.operator_src().expect("operator kind");
            let mut quoted = String::new();
            json_string(&mut quoted, &re_escape(src));
            let inner = &quoted[1..quoted.len() - 1];
            assert!(g.contains(inner), "missing operator {src}");
        }
    }

    #[test]
    fn grammar_uses_dark_plus_scopes() {
        let g = tm_grammar();
        for scope in [
            "entity.name.function.j3",
            "entity.name.type.j3",
            "entity.name.namespace.j3",
            "support.type.builtin.j3",
            "support.function.intrinsic.j3",
            "variable.other.constant.j3",
            "variable.other.property.j3",
            "variable.other.enummember.j3",
            "variable.parameter.j3",
            "keyword.control.j3",
        ] {
            assert!(g.contains(scope), "missing scope {scope}");
        }
    }

    #[test]
    fn builtin_types_are_compiler_known() {
        let cx = IrCx::new();
        for name in BUILTIN_TYPES {
            assert!(cx.builtin_named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn shipped_ui_grammar_is_dump_syntax() {
        let shipped = include_str!("../../../examples/editors/vscode/syntaxes/j3ui.tmLanguage.json");
        assert_eq!(
            ui_tm_grammar(),
            shipped,
            "run `j3c dump-syntax --ui ../examples/editors/vscode/syntaxes/j3ui.tmLanguage.json`"
        );
    }

    #[test]
    fn shipped_grammar_is_dump_syntax() {
        let shipped = include_str!("../../../examples/editors/vscode/syntaxes/j3.tmLanguage.json");
        assert_eq!(
            tm_grammar(),
            shipped,
            "run `j3c dump-syntax ../examples/editors/vscode/syntaxes/j3.tmLanguage.json`"
        );
    }
}
