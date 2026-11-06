//! `extend T { .. }` (chunk 5b of plan 030): methods outside the struct body, merged into the struct.
use super::*;

const PART: &str = "module t;\n";

/// `main.j3` plus `main/more.j3` (a part of module `t`), run on both backends.
fn run(name: &str, main: &str, part: &str, want: &str) {
    let dir = std::env::temp_dir().join(format!("j3-extend-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("main")).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, main).unwrap();
    std::fs::write(dir.join("main/more.j3"), part).unwrap();
    let prog = compile_path(&file).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want, "{backend:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Checks the given files of one project; returns the error.
fn err(files: &[(&str, &str)]) -> check::CheckError {
    let parsed = files.iter().enumerate().map(|(i, (p, s))| parse_file(i as u32 + 1, p, s)).collect();
    match check::check_files(parsed) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

fn full(e: &check::CheckError) -> String {
    format!("{}\n{}", e.message, e.helps.join("\n"))
}

#[test]
fn extend_in_part_file_runs_and_ufcs_still_works() {
    run(
        "basic",
        "module t;\nuse j3.io.printLn;\nstruct P { x: int; fn get(this: &P): int { return this.x; } }\nfn tag(p: P): int { return 1000 + p.x; }\nfn main() {\n    let p = P { x: 2 };\n    p.add(5);\n    printLn(`{p.get()} {p.twice()} {p.tag()} {p.viaPrivate()}`);\n}\n",
        "module t;\nextend P {\n    fn add(this: &P, n: int) { this.x += n; }\n    fn twice(this: &P): int { return this.get() * 2; }\n    fn viaPrivate(this: &P): int { return secret(); }\n}\nfn secret(): int { return 99; }\n",
        "7 14 1007 99\n",
    );
}

#[test]
fn extend_in_the_same_file_and_generic_struct() {
    run(
        "generic",
        "module t;\nuse j3.io.printLn;\nstruct Box<T> { v: T; }\nextend Box<T> {\n    fn first(this: &Box<T>): T { return this.v; }\n}\nfn main() {\n    let a = Box<int> { v: 3 };\n    let b = Box<string> { v: \"s\" };\n    printLn(`{a.first()} {b.first()} {a.second()}`);\n}\n",
        "module t;\nextend Box<T> {\n    fn second(this: &Box<T>): T { return this.first(); }\n}\n",
        "3 s 3\n",
    );
}

#[test]
fn error_in_an_extend_method_points_into_the_part_file() {
    let e = err(&[
        ("main.j3", "module t;\nstruct P { x: int; }\nfn main() {}\n"),
        ("main/more.j3", "module t;\nextend P {\n    fn bad(this: &P): int { return \"s\"; }\n}\n"),
    ]);
    assert_eq!(e.path, "main/more.j3", "{}", e.message);
}

#[test]
fn unknown_type() {
    let e = err(&[("a.j3", "module t;\nextend Nope { fn m(this: &Nope) {} }\n")]);
    assert!(e.message.contains("no struct `Nope`"), "{}", full(&e));
    assert!(!e.span.is_dummy());
}

#[test]
fn type_of_another_module() {
    let e = err(&[
        ("a.j3", "module a.dom;\nstruct Dom { x: int; }\n"),
        ("b.j3", "module t;\nextend Dom { fn m(this: &Dom) {} }\nfn main() {}\n"),
    ]);
    assert!(full(&e).contains("methods of `Dom` go in module a.dom: its file or a part file"), "{}", full(&e));
}

#[test]
fn enum_is_rejected() {
    let e = err(&[("a.j3", "module t;\nenum E { A, B }\nextend E { fn m(this: E) {} }\n")]);
    assert!(e.message.contains("enum `E`") && full(&e).contains("free fn"), "{}", full(&e));
}

#[test]
fn duplicate_method_across_body_and_blocks() {
    let body = err(&[("a.j3", "module t;\nstruct P { x: int; fn m(this: &P) {} }\nextend P { fn m(this: &P) {} }\n")]);
    assert!(body.message.contains("already has a method `m`"), "{}", full(&body));
    let two = err(&[
        ("a.j3", "module t;\nstruct P { x: int; }\nextend P { fn m(this: &P) {} }\n"),
        ("b.j3", "module t;\nextend P { fn m(this: &P) {} }\n"),
    ]);
    assert!(two.message.contains("already has a method `m`"), "{}", full(&two));
    assert_eq!(two.path, "b.j3");
}

#[test]
fn generic_parameters_must_match() {
    let few = err(&[("a.j3", "module t;\nstruct Box<T> { v: T; }\nextend Box { fn m(this: &Box<T>) {} }\n")]);
    assert!(few.message.contains("does not match `struct Box<T>`"), "{}", full(&few));
    let name = err(&[("a.j3", "module t;\nstruct Box<T> { v: T; }\nextend Box<U> { fn m(this: &Box<U>) {} }\n")]);
    assert!(full(&name).contains("write `extend Box<T>`"), "{}", full(&name));
}

#[test]
fn pub_and_scripts_are_rejected_with_a_hint() {
    let src = format!("{PART}struct P {{ x: int; }}\nextend P {{ pub fn m(this: &P) {{}} }}\n");
    let e = match parse::parse(&src, FileId(1)) {
        Ok(_) => panic!("expected a parse error"),
        Err(e) => e,
    };
    assert!(e.message.contains("drop `pub`"), "{}", e.message);
    let s = parse::parse_script("extend P { fn m(this: &P) {} }\n", FileId(1), &["s".to_string()]);
    assert!(s.err().is_some_and(|e| e.message.contains("scripts can't use `extend`")));
}

#[test]
fn extend_stays_an_ordinary_identifier_elsewhere() {
    run(
        "ident",
        "module t;\nuse j3.io.printLn;\nfn f(extend: int): int { return extend + 1; }\nfn main() { printLn(`{f(1)}`); }\n",
        "module t;\n",
        "2\n",
    );
}

#[test]
fn doc_comment_before_extend_is_dropped() {
    // a part file's `///` header lands on its first `extend` block (`j3c doc search` failed on this)
    let src = format!("{PART}struct P {{ x: int; }}\n/// part header\nextend P {{ fn m(this: &P) {{}} }}\n");
    assert!(parse::parse(&src, FileId(1)).is_ok());
}
