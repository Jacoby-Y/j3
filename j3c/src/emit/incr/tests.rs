//! Tests for the incremental cache keys and function chunks.

use super::*;

/// A const array's values (shader text lives in these) change the module's object key.
#[test]
fn const_global_values_change_module_key() {
    let dir = std::env::temp_dir().join(format!("j3c_incr_const_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let key = |s: &str, n: i64| {
        let path = dir.join("main.j3");
        let src = format!("module m;\nconst S: [2]string = [\"{s}\", \"x\"];\nconst N: [1]int = [{n}];\nfn main() {{ let a = S[0]; let b = N[0]; }}\n");
        fs::write(&path, src).unwrap();
        let prog = crate::compile::check_path_lib(&path).expect("check");
        let m = prog.modules.iter().find(|m| module_label(&prog.cx, m) == "m").expect("module m");
        module_ir(&prog.cx, m)
    };
    let base = key("AAA", 1);
    assert_ne!(base, key("BBB", 1));
    assert_ne!(base, key("AAA", 2));
    let _ = fs::remove_dir_all(&dir);
}

/// Vector ops and types change the module's object key (min vs max, shuffle order, vec4f vs vec4i).
#[test]
fn vector_ops_change_module_key() {
    let dir = std::env::temp_dir().join(format!("j3c_incr_simd_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let key = |body: &str| {
        let path = dir.join("main.j3");
        fs::write(&path, format!("module m;\nfn f(a: vec4f, b: vec4f, i: vec4i): vec4f {{ {body} }}\nfn main() {{}}\n")).unwrap();
        let prog = crate::compile::check_path_lib(&path).expect("check");
        let m = prog.modules.iter().find(|m| module_label(&prog.cx, m) == "m").expect("module m");
        module_ir(&prog.cx, m)
    };
    let base = key("return @vmin(a, b);");
    assert_ne!(base, key("return @vmax(a, b);"));
    assert_ne!(key("return @vshuffle(a, 0, 1, 2, 3);"), key("return @vshuffle(a, 0, 1, 3, 2);"));
    assert_ne!(base, key("return @vfloat(i);"));
    let _ = fs::remove_dir_all(&dir);
}

/// Every function is defined by exactly one chunk, and `@main` by exactly one object.
#[test]
fn chunks_define_each_function_once() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/01_basics.j3");
    let prog = crate::compile::check_path_lib(&entry).expect("check");
    let opts = EmitOpts::default();
    let names = |ll: &str| -> Vec<String> {
        ll.lines()
            .filter_map(|l| l.strip_prefix("define "))
            .map(|r| r.split('@').nth(1).unwrap_or("").split('(').next().unwrap_or("").to_string())
            .collect()
    };
    let mut mains = 0;
    for (index, module) in prog.modules.iter().enumerate() {
        let part = llvm::chunk_assign(&llvm::func_weights(&prog.cx, module), 3);
        let mut defs = Vec::new();
        for n in 0..3 {
            let set: HashSet<FuncId> = module.funcs.iter().zip(&part).filter(|(_, c)| **c == n).map(|(f, _)| *f).collect();
            let ll = llvm::emit_llvm_module(&prog.cx, &prog.modules, index, &opts, Some((n, &set))).expect("emit");
            for name in names(&ll) {
                if name == "main" {
                    mains += 1;
                } else {
                    defs.push(name);
                }
            }
        }
        let whole = llvm::emit_llvm_module(&prog.cx, &prog.modules, index, &opts, None).expect("emit");
        // Only what the module uses is declared.
        for line in whole.lines().filter(|l| l.starts_with("declare ")) {
            let name = line.split('@').nth(1).unwrap_or("").split('(').next().unwrap_or("");
            let uses = whole.matches(&format!("@{name}(")).count() + whole.matches(&format!("@{name},")).count() + whole.matches(&format!("@{name} ")).count();
            assert!(uses > 1, "unused declare in module {index}: {line}");
        }
        let mut want: Vec<String> = names(&whole).into_iter().filter(|n| n != "main").collect();
        want.sort();
        defs.sort();
        assert_eq!(defs, want);
    }
    assert_eq!(mains, 1);
}
