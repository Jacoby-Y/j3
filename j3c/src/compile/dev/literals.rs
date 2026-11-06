//! Dev lowering: script literals in native code become `devNew(path, props)`.

use super::*;

/// In a native fn, turn every script literal into `devNew(path, props)`.
pub(super) fn rewrite_literals(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    id: FuncId,
) -> Result<(), String> {
    let FuncKind::J3 { body } = &cx.func(id).kind else {
        return Ok(());
    };
    if !block_has_literal(cx, &body.block, scripts) {
        return Ok(());
    }
    let body = body.clone();
    let mut fb = Fb::with_locals(cx, &body);
    fb.params = body.params.clone();
    let block = rewrite_block(cx, rt, scripts, &mut fb, body.block)?;
    let new_body = Body {
        locals: fb.locals,
        params: fb.params,
        block,
    };
    cx.func_mut(id).kind = FuncKind::J3 { body: new_body };
    Ok(())
}

pub(super) fn block_has_literal(cx: &IrCx, block: &Block, scripts: &HashMap<StructId, ScriptInfo>) -> bool {
    block.stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::Assign { rvalue, .. } => {
            matches!(rvalue.kind, RvalueKind::Aggregate { .. })
                && script_of(cx, rvalue.ty, scripts).is_some()
        }
        StmtKind::Defer { body } | StmtKind::Block(body) | StmtKind::While { body, .. } => {
            block_has_literal(cx, body, scripts)
        }
        StmtKind::ForRange { body, .. } => block_has_literal(cx, body, scripts),
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, b)| block_has_literal(cx, b, scripts)) || block_has_literal(cx, default, scripts)
        }
        StmtKind::If {
            then_block,
            else_block,
            ..
        } => {
            block_has_literal(cx, then_block, scripts)
                || else_block
                    .as_ref()
                    .is_some_and(|b| block_has_literal(cx, b, scripts))
        }
        _ => false,
    })
}

pub(super) fn rewrite_block(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    fb: &mut Fb,
    block: Block,
) -> Result<Block, String> {
    let mut out = Vec::new();
    for mut stmt in block.stmts {
        match stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                let sid = match (&rvalue.kind, script_of(cx, rvalue.ty, scripts)) {
                    (RvalueKind::Aggregate { .. }, Some(sid)) => sid,
                    _ => {
                        out.push(Stmt {
                            kind: StmtKind::Assign { place, rvalue },
                            ..stmt
                        });
                        continue;
                    }
                };
                let RvalueKind::Aggregate { fields } = rvalue.kind else { unreachable!() };
                let saved = std::mem::take(&mut fb.stmts);
                let handle = new_instance(cx, rt, scripts, fb, sid, fields)?;
                let mut made = std::mem::replace(&mut fb.stmts, saved);
                out.append(&mut made);
                out.push(Stmt::assign(
                    stmt.span,
                    stmt.ty,
                    place,
                    Rvalue::aggregate(rvalue.span, rvalue.ty, vec![handle]),
                ));
            }
            StmtKind::Defer { body } => {
                stmt.kind = StmtKind::Defer {
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::Block(body) => {
                stmt.kind = StmtKind::Block(rewrite_block(cx, rt, scripts, fb, body)?);
                out.push(stmt);
            }
            StmtKind::While { cond, body } => {
                stmt.kind = StmtKind::While {
                    cond,
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::ForRange { index, end, body } => {
                stmt.kind = StmtKind::ForRange {
                    index,
                    end,
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let then_block = rewrite_block(cx, rt, scripts, fb, then_block)?;
                let else_block = match else_block {
                    Some(b) => Some(rewrite_block(cx, rt, scripts, fb, b)?),
                    None => None,
                };
                stmt.kind = StmtKind::If {
                    cond,
                    then_block,
                    else_block,
                };
                out.push(stmt);
            }
            StmtKind::Switch { scrut, cases, default } => {
                let mut new_cases = Vec::new();
                for (v, b) in cases {
                    new_cases.push((v, rewrite_block(cx, rt, scripts, fb, b)?));
                }
                let default = rewrite_block(cx, rt, scripts, fb, default)?;
                stmt.kind = StmtKind::Switch { scrut, cases: new_cases, default };
                out.push(stmt);
            }
            kind => {
                stmt.kind = kind;
                out.push(stmt);
            }
        }
    }
    Ok(Block { stmts: out })
}

/// Emit `devNew(path, [props the literal named])` and return the handle operand.
pub(super) fn new_instance(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    fb: &mut Fb,
    sid: StructId,
    fields: Vec<Operand>,
) -> Result<Operand, String> {
    let info = &scripts[&sid];
    let path = info.path.clone();
    let script_file = info.file;
    let props = info.props.clone();
    let prop_ty = cx.ty_struct(rt.prop);
    let int = cx.common.int;
    let mut items = Vec::new();
    for (i, (name, ty)) in props.iter().enumerate() {
        let Some(value) = fields.get(i).cloned() else { continue };
        if operand_span(&value).file == script_file {
            // The script's own default; the VM fills it from the bytecode.
            continue;
        }
        let (addr, size) = vm_arg(cx, fb, scripts, value, *ty)?;
        let name = string_const(cx, name);
        items.push(fb.value(
            cx,
            Rvalue::aggregate(
                Span::dummy(),
                prop_ty,
                vec![name, addr, Operand::int(Span::dummy(), int, size)],
            ),
        ));
    }
    let props = slice_of(cx, fb, prop_ty, items);
    let callee = cx.func(rt.dev_new).ty;
    let path = string_const(cx, &path);
    Ok(fb.value(
        cx,
        Rvalue::call(
            Span::dummy(),
            cx.common.ptr,
            Operand::fn_addr(Span::dummy(), callee, rt.dev_new),
            vec![path, props],
        ),
    ))
}

pub(super) fn operand_span(op: &Operand) -> Span {
    match op {
        Operand::Const(c) => c.span,
        Operand::Copy(place) => place.span,
    }
}
