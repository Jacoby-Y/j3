//! Source locations. Parser will fill these in; hand-built IR uses `Span::dummy`.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn dummy() -> Span {
        Span {
            file: FileId(0),
            start: 0,
            end: 0,
        }
    }

    pub fn is_dummy(self) -> bool {
        self.file.0 == 0 && self.start == 0 && self.end == 0
    }
}
