//! Scene scripts in lowering: a script instance used by value, `scene.script<T>()` lookup, and
//! the `script` property a place reads through.

use super::*;

impl<'a> Lower<'a> {
    /// A script instance used by value. Generic code (`List<Door>` growing) may move one;
    /// user code may not copy one.
    pub(super) fn is_script_value(&self, ty: Ty) -> bool {
        if !self.type_subst.is_empty() {
            return false;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) => self.cx.struct_def(*sid).script.is_some(),
            _ => false,
        }
    }

    /// `j3.scene3d.Scene`.
    pub(super) fn is_scene(&self, sid: StructId) -> bool {
        let def = self.cx.struct_def(sid);
        self.cx.name_str(def.name) == "Scene"
            && def.module.len() == 2
            && self.cx.name_str(def.module[0]) == "j3"
            && self.cx.name_str(def.module[1]) == "scene3d"
    }

    /// `scene.script<Door>(node)`: the registry's `find` for that script type.
    pub(super) fn scene_script_find(&self, ty: Ty, span: Span) -> Result<FuncId, CheckError> {
        let script = match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) if self.cx.struct_def(*sid).script.is_some() => *sid,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "`scene.script<T>` takes a script type",
                )
                .help("bind one with `const Door = @script(\"scripts/door.j3s\");`"));
            }
        };
        let path = crate::emit::bc::script_path(self.cx, &self.cx.struct_def(script).module);
        let want = format!("find__{}", crate::compile::scene_scripts::ident(&path));
        let registry = |f: &crate::ir::entity::Func| {
            f.module.len() == 3
                && self.cx.name_str(f.module[0]) == "j3"
                && self.cx.name_str(f.module[1]) == "scene3d"
                && self.cx.name_str(f.module[2]) == "scripts"
        };
        self.cx
            .funcs()
            .iter()
            .find(|f| registry(f) && self.cx.name_str(f.name) == want)
            .map(|f| f.id)
            .ok_or_else(|| {
                CheckError::new(
                    self.path,
                    span,
                    format!("no scene attaches {path}, so no object has one"),
                )
                .help(format!(
                    "attach it to an object in the editor (`script={path}` on its line)"
                ))
            })
    }

    pub(super) fn script_copy(&self, span: Span, ty: Ty) -> CheckError {
        let name = match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) => self.cx.name_str(self.cx.struct_def(*sid).name).to_string(),
            _ => "the script".to_string(),
        };
        CheckError::new(self.path, span, "script instances can't be copied")
            .help(format!("pass `&{name}` instead"))
    }

    pub(super) fn script_prop_name<'b>(&self, expr: &'b Expr) -> Option<&'b str> {
        match &expr.kind {
            ExprKind::Name(n) if self.script_props.contains(&n.name) => Some(&n.name),
            ExprKind::Field { base, field } => {
                if matches!(&base.kind, ExprKind::Name(n) if n.name == "this")
                    && self.script_props.contains(&field.name)
                {
                    Some(&field.name)
                } else {
                    self.script_prop_name(base)
                }
            }
            ExprKind::Index { base, .. } => self.script_prop_name(base),
            _ => None,
        }
    }
}
