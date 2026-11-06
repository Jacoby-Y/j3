//! `extend T { .. }` blocks folded into their structs, module by module (`parse::extend`).

use super::*;

pub(super) fn merge_extends(mods: &mut [ModData]) -> Result<(), CheckError> {
    for i in 0..mods.len() {
        let mut items = std::mem::take(&mut mods[i].ast.items);
        let r = parse::extend::merge(&mut items, &mods[i].key, &|name| {
            mods.iter().enumerate().find_map(|(j, o)| {
                let declares = j != i && o.ast.items.iter().any(|it| match it {
                    Item::Struct(s) => s.name.name == name,
                    Item::Enum(e) => e.name.name == name,
                    _ => false,
                });
                declares.then(|| o.key.clone())
            })
        });
        mods[i].ast.items = items;
        if let Err(e) = r {
            return Err(CheckError::new(mods[i].path.clone(), e.span, e.message).help(e.help));
        }
    }
    Ok(())
}
