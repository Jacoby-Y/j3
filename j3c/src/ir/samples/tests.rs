//! Tests for the hand-built place samples: validate, dump, emit C and run.

use super::*;
use crate::ir::dump::dump_module;
use crate::ir::validate::validate_module;

#[test]
fn place_samples_validate() {
    let samples = place_samples();
    match validate_module(&samples.cx, &samples.module) {
        Ok(()) => {}
        Err(e) => panic!("{}", e.message),
    }
}

#[test]
fn place_samples_dump_mentions_deref_and_assign_op() {
    let samples = place_samples();
    let dump = dump_module(&samples.cx, &samples.module);
    assert!(dump.contains("(*this).x *= copy s"), "{dump}");
    assert!(dump.contains("xs[copy i] += 1"), "{dump}");
    assert!(dump.contains("&(*(*g).mem)"), "{dump}");
    assert!(dump.contains("(*g).running = false"), "{dump}");
}

#[test]
fn emit_c_compiles_and_run_exits_zero() {
    let samples = place_samples();
    match validate_module(&samples.cx, &samples.module) {
        Ok(()) => {}
        Err(e) => panic!("{}", e.message),
    }
    let c = crate::emit::emit_c(&samples.cx, &samples.module).expect("emit c");
    assert!(c.contains("j3_ir_samples_scale_"), "{c}");
    assert!(c.contains("j3_sl_i64"), "{c}");
    assert!(c.contains("j3_ir_samples_Vec2"), "{c}");
    assert!(c.contains("int main(int argc, char **argv)"), "{c}");
    let run = crate::emit::compile_and_run(&c).expect("cc");
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let mut opts = crate::emit::EmitOpts::default();
    opts.backend = crate::emit::Backend::Llvm;
    if !crate::emit::clang_available(&opts.clang) {
        opts.backend = crate::emit::Backend::C;
    }
    let ll_run = crate::emit::run_program(&samples.cx, std::slice::from_ref(&samples.module), &opts)
        .expect("llvm run");
    assert_eq!(
        ll_run.status,
        0,
        "llvm stderr {}",
        String::from_utf8_lossy(&ll_run.stderr)
    );
}
