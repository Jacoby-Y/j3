use super::*;

/// Receiver adjustment shared by methods and UFCS calls (`this_adjust`).
pub(super) enum ThisAdjust {
    Same,
    TakeRef,
    Deref,
}

impl<'a> Lower<'a> {
    /// `x.f(args)` as `f(x, args)`: a fn visible by bare name `f` whose first parameter takes the
    /// receiver, when the receiver's type has no method `f` (the type's own method wins).
    /// ponytail: `funcs` holds one fn per name, so two visible matches cannot occur.
    pub(super) fn ufcs_target(&self, sid: Option<StructId>, name: &str, recv: Ty) -> Option<FuncId> {
        if let Some(sid) = sid {
            let key = (sid.0, name.to_string());
            let scene_script = name == "script" && self.is_scene(sid);
            if self.methods.contains_key(&key) || self.method_temps.contains_key(&key) || scene_script {
                return None;
            }
        }
        let id = self.funcs.get(name).copied()?;
        let first = self.first_param(id)?;
        self.this_adjust(recv, first).map(|_| id)
    }

    fn first_param(&self, id: FuncId) -> Option<Ty> {
        self.cx.fn_sig(self.cx.func(id).ty)?.0.first().copied()
    }

    /// Adds "or a visible fn" help to a no-method error when a fn of that name exists.
    pub(super) fn ufcs_hint(&self, e: CheckError, name: &str) -> CheckError {
        let first = match self.funcs.get(name).and_then(|&id| self.first_param(id)) {
            Some(t) => t,
            None => return e,
        };
        e.help(format!(
            "or a visible fn `{name}` whose first parameter takes the receiver (it takes `{}`)",
            self.ty_str(first)
        ))
    }
}
