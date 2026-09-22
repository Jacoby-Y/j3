//! Interned names. Cheap to copy, compared by id.

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Name(pub u32);

#[derive(Clone, Debug)]
pub struct Names {
    strings: Vec<String>,
    map: HashMap<String, Name>,
}

impl Names {
    pub fn new() -> Names {
        Names {
            strings: Vec::new(),
            map: HashMap::new(),
        }
    }

    pub fn intern(&mut self, s: &str) -> Name {
        if let Some(&name) = self.map.get(s) {
            return name;
        }
        let name = Name(self.strings.len() as u32);
        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), name);
        name
    }

    pub fn get(&self, name: Name) -> &str {
        &self.strings[name.0 as usize]
    }
}

impl Default for Names {
    fn default() -> Names {
        Names::new()
    }
}
