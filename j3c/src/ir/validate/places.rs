//! Validation of places: projection chains, types and `Access` folding.

use super::*;

pub(super) fn validate_place(cx: &IrCx, body: &Body, place: &Place) -> Result<(), ValidateError> {
    let (mut ty, access0) = match place.root {
        PlaceRoot::Local(id) => {
            let decl = local(body, id)?;
            (decl.ty, decl.access)
        }
        PlaceRoot::Global(id) => {
            let g = cx.global(id);
            (g.ty, if g.readonly { Access::read_only_view() } else { Access::let_binding() })
        }
    };

    for proj in &place.proj {
        match proj {
            Proj::Deref => match cx.ref_inner(ty) {
                Some(inner) => {
                    ty = inner;
                }
                None => {
                    return Err(ValidateError::new(
                        "Deref projection on non-ref type".to_string(),
                    ));
                }
            },
            Proj::Field {
                field,
                ty: field_ty,
            } => match cx.ty_kind(ty) {
                TyKind::Struct(sid) => {
                    if field.struct_id != *sid {
                        return Err(ValidateError::new(
                            "field id does not match struct".to_string(),
                        ));
                    }
                    let def = cx.field_def(*field);
                    if def.ty != *field_ty {
                        return Err(ValidateError::new(
                            "field projection ty != field def".to_string(),
                        ));
                    }
                    ty = def.ty;
                }
                _ => {
                    return Err(ValidateError::new(
                        "field projection on non-struct".to_string(),
                    ));
                }
            },
            Proj::Index { index, ty: elem_ty } => {
                validate_operand(cx, body, index)?;
                if index.ty() != cx.common.int {
                    return Err(ValidateError::new("index is not int".to_string()));
                }
                match cx.elem_ty(ty) {
                    Some(elem) => {
                        if elem != *elem_ty {
                            return Err(ValidateError::new(
                                "index projection ty != elem ty".to_string(),
                            ));
                        }
                        ty = elem;
                    }
                    None => {
                        return Err(ValidateError::new(
                            "index projection on non-array/slice/string".to_string(),
                        ));
                    }
                }
            }
            Proj::Ptr => match cx.elem_ty(ty) {
                Some(elem) => match cx.lookup_ty(&TyKind::Ref(elem)) {
                    Some(ptr_ty) => {
                        ty = ptr_ty;
                    }
                    None => {
                        return Err(ValidateError::new(
                            "slice ptr type &T was never interned".to_string(),
                        ));
                    }
                },
                None => {
                    return Err(ValidateError::new(
                        "Ptr projection on non-array/slice/string".to_string(),
                    ));
                }
            },
            Proj::Len => match cx.elem_ty(ty) {
                Some(_) => {
                    ty = cx.common.int;
                }
                None => {
                    return Err(ValidateError::new(
                        "Len projection on non-array/slice/string".to_string(),
                    ));
                }
            },
            Proj::Lane { lane, ty: lane_ty } => match cx.vec_lane(ty) {
                Some(l) if l == *lane_ty && *lane < 4 => ty = l,
                _ => return Err(ValidateError::new("Lane projection on non-vector or bad lane".to_string())),
            },
            Proj::EnumTag => match cx.ty_kind(ty) {
                TyKind::Enum(id) if !cx.enum_def(*id).is_c_like() => {
                    ty = cx.common.i32;
                }
                _ => {
                    return Err(ValidateError::new("EnumTag on non-tagged enum".to_string()));
                }
            },
            Proj::EnumPayload { variant, ty: pty } => match cx.ty_kind(ty) {
                TyKind::Enum(id) if *id == variant.enum_id => {
                    let def = cx.enum_def(*id);
                    let v = &def.variants[variant.index as usize];
                    match v.payload {
                        Some(p) if p == *pty => ty = p,
                        _ => {
                            return Err(ValidateError::new(
                                "EnumPayload does not match variant".to_string(),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(ValidateError::new("EnumPayload on wrong type".to_string()));
                }
            },
        }
    }

    let access = fold_access(
        cx,
        access0,
        &place.proj,
        match place.root {
            PlaceRoot::Local(id) => local(body, id)?.ty,
            PlaceRoot::Global(id) => cx.global(id).ty,
        },
    );

    if ty != place.ty {
        return Err(ValidateError::new(
            "place.ty does not match projections".to_string(),
        ));
    }
    if access.assign != place.access.assign || access.take_ref != place.access.take_ref {
        return Err(ValidateError::new(
            "place.access does not match projections".to_string(),
        ));
    }
    Ok(())
}

fn fold_access(cx: &IrCx, start: Access, projs: &[Proj], start_ty: Ty) -> Access {
    let mut access = start;
    let mut ty = start_ty;
    for proj in projs {
        match proj {
            Proj::Deref => {
                if let Some(inner) = cx.ref_inner(ty) {
                    ty = inner;
                }
                access = Access::through_ref();
            }
            Proj::Field { field, ty: field_ty } => {
                if cx.struct_def(field.struct_id).script.as_ref()
                    .is_some_and(|meta| (field.index as usize) < meta.prop_spans.len()) {
                    access = Access::read_only_view();
                }
                ty = *field_ty;
            }
            Proj::Index { ty: elem_ty, .. } => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                ty = *elem_ty;
            }
            Proj::Ptr => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                if let Some(elem) = cx.elem_ty(ty) {
                    if let Some(ptr_ty) = cx.lookup_ty(&TyKind::Ref(elem)) {
                        ty = ptr_ty;
                    }
                }
            }
            Proj::Len => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                ty = cx.common.int;
            }
            Proj::EnumTag => {
                ty = cx.common.i32;
            }
            Proj::Lane { ty: lane_ty, .. } => {
                ty = *lane_ty;
            }
            Proj::EnumPayload { ty: pty, .. } => {
                ty = *pty;
            }
        }
    }
    access
}

