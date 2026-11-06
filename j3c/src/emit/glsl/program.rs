//! One stage's GLSL text in a fixed section order, so the same program always prints the same bytes.

#[derive(Default, Clone, Debug)]
pub struct StageText {
    /// Printed as a `//` comment after `#version` (which program and stage it is).
    pub title: String,
    pub structs: Vec<String>,
    pub io: Vec<String>,
    pub uniforms: Vec<String>,
    pub consts: Vec<String>,
    pub fns: Vec<String>,
    pub main: String,
}

impl StageText {
    pub fn render(&self) -> String {
        let mut out = String::from("#version 330 core\n");
        out.push_str(&format!("// {}\n", self.title));
        for sec in [&self.structs, &self.io, &self.uniforms, &self.consts, &self.fns] {
            for item in sec {
                out.push_str(item);
                if !item.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        out.push_str(&self.main);
        out
    }
}
