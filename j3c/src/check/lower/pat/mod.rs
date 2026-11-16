//! `match` lowering. Each pattern becomes a test (a bool operand, or none when it always
//! matches) and a list of bindings (a name and the place it reads). Arms become an `If` chain,
//! or one `Switch` for an int match whose arms are single values with no guards. A guarded
//! match keeps a `done` flag so a failing guard falls through to later arms.
//! The checks (exhaustiveness, int values, overlap) are in `check.rs`; one pattern's test and bindings in `test.rs`.
//! A `match` used as a value (`ExprKind::Match`, `if let`) lowers the same way with an
//! `ArmSink`: each arm's tail is assigned to one temp, and an arm without one must leave.

use super::*;

mod check;
mod test;

/// A name a pattern binds: the value at `place`.
pub(super) struct Binding {
    pub(super) name: Ident,
    pub(super) place: Place,
}

/// Where a value `match` puts its arms' values: `slot` is the temp (made by the first arm with
/// a value, whose tail span it keeps for the mismatch error); `want` is the expected type.
pub(super) struct ArmSink {
    want: Option<Ty>,
    slot: Option<(Place, Span)>,
}

impl<'a> Lower<'a> {
    /// `match` as a value: the temp its arms assign (`if`-expression rules, a leaving arm fits).
    pub(super) fn match_value(&mut self, s: &parse::MatchStmt, expected: Option<Ty>) -> Result<Rvalue, CheckError> {
        let mut sink = ArmSink { want: expected, slot: None };
        self.lower_match(s, Some(&mut sink))?;
        match sink.slot {
            Some((place, _)) => Ok(Rvalue::use_op(s.span, Operand::Copy(place))),
            None => Err(CheckError::new(self.path, s.span, "every arm of this `match` leaves, so it has no value")),
        }
    }

    pub(super) fn lower_match(&mut self, s: &parse::MatchStmt, mut sink: Option<&mut ArmSink>) -> Result<(), CheckError> {
        let op = self.operand(&s.expr, None)?;
        self.check_arm_order(&s.arms)?;
        if let TyKind::Int(int_ty) = *self.cx.ty_kind(op.ty()) {
            let values = self.check_int_arms(s, op.ty(), int_ty)?;
            if let Some(values) = values {
                return self.lower_switch(s, op, values, sink);
            }
        } else if op.ty() == self.cx.common.string {
            self.check_str_arms(s)?;
        } else if self.cx.tuple_elems(op.ty()).is_some() {
            self.check_exhaustive(op.ty(), &s.arms, s.span)?;
        } else {
            let enum_id = self.enum_id_of(op.ty(), s.span, "match")?;
            let ety = self.cx.ty_enum(enum_id);
            self.check_exhaustive(ety, &s.arms, s.span)?;
        }
        let scrut = self.materialize(op, s.span);
        if s.arms.iter().any(|a| a.guard.is_some()) {
            return self.lower_guarded_chain(&scrut, &s.arms, sink);
        }
        let mut else_block = None;
        for arm in s.arms.iter().rev() {
            let saved = self.b.begin_block();
            let (cond, body) = self.lower_arm(&scrut, arm, None, sink.as_deref_mut())?;
            match cond {
                Some(cond) => self.push_if(arm.span, cond, body, else_block),
                // an arm that always matches (`_`) is last (`check_arm_order`)
                None => self.push_stmt(arm.span, crate::ir::stmt::StmtKind::Block(body)),
            }
            else_block = Some(self.b.end_block(saved));
        }
        if let Some(block) = else_block {
            for stmt in block.stmts {
                self.b.push(stmt);
            }
        }
        Ok(())
    }

    /// `done = false; if !done { if test1 { bind; if guard { done = true; body } } } ...`
    fn lower_guarded_chain(&mut self, scrut: &Place, arms: &[MatchArm], mut sink: Option<&mut ArmSink>) -> Result<(), CheckError> {
        let span = arms[0].span;
        let n = self.fresh("matched");
        let id = self.b.tmp(n, self.cx.common.bool, span);
        let done = self.b.local_place(id, span);
        self.b.assign(span, done.clone(), Rvalue::use_op(span, Operand::bool_const(span, self.cx.common.bool, false)));
        for arm in arms {
            let saved = self.b.begin_block();
            let (cond, body) = self.lower_arm(scrut, arm, Some(&done), sink.as_deref_mut())?;
            match cond {
                Some(cond) => self.push_if(arm.span, cond, body, None),
                None => self.push_stmt(arm.span, crate::ir::stmt::StmtKind::Block(body)),
            }
            let inner = self.b.end_block(saved);
            let not_done =
                self.eq_ops(arm.span, Operand::Copy(done.clone()), Operand::bool_const(arm.span, self.cx.common.bool, false));
            self.push_if(arm.span, not_done, inner, None);
        }
        Ok(())
    }

    /// One arm: its test (in the current block) and its body block, which binds the pattern's
    /// names, checks the guard, sets `done` (when given) and runs the arm's statements.
    fn lower_arm(
        &mut self,
        scrut: &Place,
        arm: &MatchArm,
        done: Option<&Place>,
        sink: Option<&mut ArmSink>,
    ) -> Result<(Option<Operand>, crate::ir::body::Block), CheckError> {
        let mut tests = Vec::new();
        let mut binds = Vec::new();
        for pat in &arm.pats {
            tests.push(self.pat_test(scrut, pat)?);
            let mut b = Vec::new();
            self.pat_binds(scrut, pat, &mut b)?;
            binds.push(b);
        }
        self.check_or_binds(&arm.pats, &mut binds)?;
        let mut cond = None;
        if tests.iter().all(Option::is_some) {
            for t in tests.iter().flatten() {
                cond = Some(match cond {
                    None => t.clone(),
                    Some(c) => self.bind_bool(arm.span, Rvalue::binary(arm.span, self.cx.common.bool, BinOp::Or, c, t.clone())),
                });
            }
        }
        let saved_locals = self.locals.clone();
        let saved_aliases = self.aliases.clone();
        let saved = self.b.begin_block();
        self.bind_arm(&tests, &binds)?;
        match &arm.guard {
            Some(g) => {
                let cond = self.operand(g, None)?;
                if cond.ty() != self.cx.common.bool {
                    return Err(CheckError::new(self.path, g.span, format!("a match guard is a `bool`, found `{}`", self.ty_str(cond.ty()))));
                }
                let inner = self.b.begin_block();
                self.arm_body(arm, done, sink)?;
                let then_block = self.b.end_block(inner);
                self.push_if(g.span, cond, then_block, None);
            }
            None => self.arm_body(arm, done, sink)?,
        }
        let block = self.b.end_block(saved);
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        Ok((cond, block))
    }

    fn arm_body(&mut self, arm: &MatchArm, done: Option<&Place>, sink: Option<&mut ArmSink>) -> Result<(), CheckError> {
        if let Some(done) = done {
            let s = arm.span;
            self.b.assign(s, done.clone(), Rvalue::use_op(s, Operand::bool_const(s, self.cx.common.bool, true)));
        }
        self.arm_stmts(arm, sink)
    }

    /// An arm's statements, then its tail: an expression statement in a statement `match`, the
    /// value put in `sink` in a value one (where an arm with no value must leave).
    fn arm_stmts(&mut self, arm: &MatchArm, sink: Option<&mut ArmSink>) -> Result<(), CheckError> {
        for stmt in &arm.body.stmts {
            self.stmt(stmt)?;
        }
        let Some(sink) = sink else {
            return match &arm.tail {
                Some(t) => self.expr_stmt(t),
                None => Ok(()),
            };
        };
        match &arm.tail {
            Some(t) if !crate::parse::flow::expr_diverges(t) => {
                let want = sink.slot.as_ref().map(|(p, _)| p.ty).or(sink.want);
                let rv = self.rvalue(t, want)?;
                self.sink_put(sink, rv, t.span)
            }
            Some(t) => self.expr_stmt(t),
            None if crate::parse::flow::leaves(&arm.body) => Ok(()),
            None => Err(CheckError::new(self.path, arm.span, "this arm gives no value")
                .help("end it with an expression and no `;`, or leave with `return`, `break`, `continue` or `panic`")),
        }
    }

    fn sink_put(&mut self, sink: &mut ArmSink, rv: Rvalue, span: Span) -> Result<(), CheckError> {
        if rv.ty == self.cx.common.void {
            return Err(CheckError::new(self.path, span, "this arm's value is `void`").help("a `match` used as a value needs a value from every arm that does not leave"));
        }
        match &sink.slot {
            Some((place, first)) => {
                if rv.ty != place.ty {
                    // arms lower last to first: the error sits on the later one in the source
                    let (here, here_ty, there, there_ty) =
                        if span.start > first.start { (span, rv.ty, *first, place.ty) } else { (*first, place.ty, span, rv.ty) };
                    return Err(CheckError::new(
                        self.path,
                        here,
                        format!("match arms have different types: this arm is `{}`, the arm at {} is `{}`", self.ty_str(here_ty), self.line_of(there), self.ty_str(there_ty)),
                    ));
                }
                let place = place.clone();
                self.b.assign(span, place, rv);
            }
            None => {
                let n = self.fresh("match");
                let id = self.b.tmp(n, rv.ty, span);
                let place = self.b.local_place(id, span);
                self.b.assign(span, place.clone(), rv);
                sink.slot = Some((place, span));
            }
        }
        Ok(())
    }

    /// Declares the arm's names and copies them in from whichever alternative matched.
    fn bind_arm(&mut self, tests: &[Option<Operand>], binds: &[Vec<Binding>]) -> Result<(), CheckError> {
        if binds[0].is_empty() {
            return Ok(());
        }
        let mut dests = Vec::new();
        for b in &binds[0] {
            let n = &b.name;
            if self.is_declared(&n.name) || self.aliases.contains_key(&n.name) {
                return Err(CheckError::new(self.path, n.span, format!("shadowing `{}` is a compile error", n.name)));
            }
            let id = self.b.let_local(self.cx.intern_name(&n.name), b.place.ty, n.span);
            self.locals.insert(n.name.clone(), id);
            dests.push(self.b.local_place(id, n.span));
        }
        if binds.len() == 1 {
            for (b, dest) in binds[0].iter().zip(&dests) {
                self.b.assign(b.name.span, dest.clone(), Rvalue::use_op(b.name.span, Operand::Copy(b.place.clone())));
            }
            return Ok(());
        }
        // the last alternative needs no test: one of them matched
        let mut else_block: Option<crate::ir::body::Block> = None;
        for (k, alt) in binds.iter().enumerate().rev() {
            let saved = self.b.begin_block();
            for (b, dest) in alt.iter().zip(&dests) {
                self.b.assign(b.name.span, dest.clone(), Rvalue::use_op(b.name.span, Operand::Copy(b.place.clone())));
            }
            let assigns = self.b.end_block(saved);
            let saved = self.b.begin_block();
            match (&tests[k], k + 1 < binds.len()) {
                (Some(t), true) => self.push_if(alt[0].name.span, t.clone(), assigns, else_block),
                _ => self.push_stmt(alt[0].name.span, crate::ir::stmt::StmtKind::Block(assigns)),
            }
            else_block = Some(self.b.end_block(saved));
        }
        for stmt in else_block.map(|b| b.stmts).unwrap_or_default() {
            self.b.push(stmt);
        }
        Ok(())
    }

    /// `let Span { lo, hi } = s;`: a pattern that always matches, then its bindings.
    pub(super) fn lower_let_pat(&mut self, s: &parse::LetPatStmt) -> Result<(), CheckError> {
        if let Some(else_block) = &s.else_block {
            return self.lower_let_else(s, else_block);
        }
        if !s.pat.irrefutable() {
            return Err(CheckError::new(self.path, s.pat.span(), "this `let` pattern may not match")
                .help("a `let` pattern is names, `_` and struct patterns; use `match` for a variant or a value"));
        }
        let op = self.operand(&s.value, None)?;
        let scrut = self.materialize(op, s.span);
        let test = self.pat_test(&scrut, &s.pat)?;
        debug_assert!(test.is_none(), "an irrefutable pattern has no test");
        let mut binds = vec![Vec::new()];
        self.pat_binds(&scrut, &s.pat, &mut binds[0])?;
        self.check_or_binds(std::slice::from_ref(&s.pat), &mut binds)?;
        self.bind_arm(&[None], &binds)
    }

    /// `let E.A(x) = e else { return; };`: `if !test { else }`, then the bindings, so they are
    /// in scope after the statement and not in the `else`.
    fn lower_let_else(&mut self, s: &parse::LetPatStmt, else_block: &parse::Block) -> Result<(), CheckError> {
        if s.pat.irrefutable() {
            return Err(CheckError::new(self.path, s.pat.span(), "this `let` pattern always matches, so the `else` never runs")
                .help("drop the `else`"));
        }
        if !crate::parse::flow::leaves(else_block) {
            return Err(CheckError::new(self.path, else_block.span, "the `else` of `let .. else` must leave")
                .help("end it with `return`, `break`, `continue` or `panic`"));
        }
        let op = self.operand(&s.value, None)?;
        let scrut = self.materialize(op, s.span);
        if let Some(test) = self.pat_test(&scrut, &s.pat)? {
            let span = else_block.span;
            let failed = self.eq_ops(span, test, Operand::bool_const(span, self.cx.common.bool, false));
            let body = self.lower_block(else_block)?;
            self.push_if(span, failed, body, None);
        }
        let mut binds = vec![Vec::new()];
        self.pat_binds(&scrut, &s.pat, &mut binds[0])?;
        self.check_or_binds(std::slice::from_ref(&s.pat), &mut binds)?;
        self.bind_arm(&[None], &binds)
    }

    /// `A(x) | B(x)`: every alternative binds the same names with the same types. Sorts each
    /// alternative's bindings by name so they line up.
    fn check_or_binds(&self, pats: &[Pat], binds: &mut [Vec<Binding>]) -> Result<(), CheckError> {
        for b in binds.iter_mut() {
            b.sort_by(|x, y| x.name.name.cmp(&y.name.name));
            if let Some(w) = b.windows(2).find(|w| w[0].name.name == w[1].name.name) {
                return Err(CheckError::new(self.path, w[1].name.span, format!("`{}` is bound twice in one pattern", w[1].name.name)));
            }
        }
        let names = |b: &[Binding]| b.iter().map(|x| x.name.name.clone()).collect::<Vec<_>>();
        for (k, b) in binds.iter().enumerate().skip(1) {
            if names(b) != names(&binds[0]) {
                return Err(CheckError::new(
                    self.path,
                    pats[k].span(),
                    format!("this alternative binds {}, the first binds {}", name_list(&names(b)), name_list(&names(&binds[0]))),
                )
                .help("every `|` alternative binds the same names"));
            }
            for (x, y) in b.iter().zip(&binds[0]) {
                if x.place.ty != y.place.ty {
                    return Err(CheckError::new(
                        self.path,
                        x.name.span,
                        format!("`{}` is `{}` here but `{}` in the first alternative", x.name.name, self.ty_str(x.place.ty), self.ty_str(y.place.ty)),
                    ));
                }
            }
        }
        Ok(())
    }

    /// `match n { 1 { } -2 { } LIMIT { } _ { } }` with single values and no guards: one `Switch`.
    /// `values[i]` is arm `i`'s value (`None` for `_`).
    fn lower_switch(&mut self, s: &parse::MatchStmt, scrut: Operand, values: Vec<Option<i128>>, mut sink: Option<&mut ArmSink>) -> Result<(), CheckError> {
        let mut cases = Vec::new();
        let mut default = None;
        for (arm, v) in s.arms.iter().zip(values) {
            let block = self.int_arm_block(arm, sink.as_deref_mut())?;
            match v {
                Some(v) => cases.push((v as u64, block)),
                None => default = Some(block),
            }
        }
        let default = default.expect("check_int_arms requires `_`");
        self.push_stmt(s.span, crate::ir::stmt::StmtKind::Switch { scrut, cases, default });
        Ok(())
    }

    fn int_arm_block(&mut self, arm: &MatchArm, sink: Option<&mut ArmSink>) -> Result<crate::ir::body::Block, CheckError> {
        let saved_locals = self.locals.clone();
        let saved_aliases = self.aliases.clone();
        let saved = self.b.begin_block();
        self.arm_stmts(arm, sink)?;
        let block = self.b.end_block(saved);
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        Ok(block)
    }

    fn push_if(&mut self, span: Span, cond: Operand, then_block: crate::ir::body::Block, else_block: Option<crate::ir::body::Block>) {
        self.push_stmt(span, crate::ir::stmt::StmtKind::If { cond, then_block, else_block });
    }

    fn push_stmt(&mut self, span: Span, kind: crate::ir::stmt::StmtKind) {
        self.b.push(crate::ir::stmt::Stmt { ty: self.cx.common.void, span, kind });
    }
}

fn name_list(names: &[String]) -> String {
    if names.is_empty() {
        return "no names".to_string();
    }
    names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}
