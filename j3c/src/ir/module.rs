//! A compiled module. Item bodies live on [`crate::ir::entity::Func`] in the cx.

use crate::intern::Name;
use crate::ir::ids::{EnumId, FuncId, GlobalId, StructId};

#[derive(Clone, Debug)]
pub struct Module {
    pub name: Vec<Name>,
    pub structs: Vec<StructId>,
    pub enums: Vec<EnumId>,
    pub funcs: Vec<FuncId>,
    pub globals: Vec<GlobalId>,
}

impl Module {
    pub fn new(name: Vec<Name>) -> Module {
        Module {
            name,
            structs: Vec::new(),
            enums: Vec::new(),
            funcs: Vec::new(),
            globals: Vec::new(),
        }
    }
}
