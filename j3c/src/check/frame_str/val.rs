//! What a value may carry, for [`super::flow`]: a set of [`Tok`]s, kept per first-level field for
//! a struct value so a local struct holding a `&Css` and a frame string does not mix the two.

/// One thing a value may carry. A param's data and a pointer to a local are narrowed by the first
/// struct field read below them (`this.name` is param `this` through field `name`), one level only.
/// A narrowed param token is the value in that field, or with [`Tok::addr`] the field's address
/// (`&this.name`); a narrowed local token is always the field's address (`&p.name`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(super) struct Tok(u64);

const K_PARAM: u64 = 0;
const K_LOC: u64 = 1;
const K_F: u64 = 2;
const K_U: u64 = 3;
const FIELD_MASK: u64 = (1 << 29) - 1;
const ADDR: u64 = 1 << 61;

impl Tok {
    /// Memory of unknown origin (a global, an allocator, a C call): a store there is kept.
    pub const U: Tok = Tok(K_U << 62);

    pub fn param(j: usize) -> Tok {
        Tok(K_PARAM << 62 | j as u64)
    }

    /// Frame interpolation buffer `x` (a local).
    pub fn buf(x: u32) -> Tok {
        Tok(K_F << 62 | x as u64)
    }

    /// The frame buffer local this points into.
    pub fn frame(self) -> Option<u32> {
        (self.0 >> 62 == K_F).then_some(self.0 as u32)
    }

    pub fn loc(x: u32) -> Tok {
        Tok(K_LOC << 62 | x as u64)
    }

    pub fn is_param(self) -> bool {
        self.0 >> 62 == K_PARAM
    }

    /// The local this points into.
    pub fn as_loc(self) -> Option<u32> {
        (self.0 >> 62 == K_LOC).then_some(self.0 as u32)
    }

    pub fn id(self) -> usize {
        self.0 as u32 as usize
    }

    pub fn field(self) -> Option<u32> {
        let f = (self.0 >> 32) & FIELD_MASK;
        (f != 0).then(|| f as u32 - 1)
    }

    /// Narrowed to field `f`, unless already narrowed (deeper fields are not tracked).
    pub fn refine(self, f: u32) -> Tok {
        let k = self.0 >> 62;
        if (k == K_PARAM || k == K_LOC) && self.field().is_none() && (f as u64) < FIELD_MASK {
            Tok(self.0 | (f as u64 + 1) << 32)
        } else {
            self
        }
    }

    /// A pointer to field `f` of what this pointer points at (`&this.f` from `this`).
    pub fn addr(self, f: u32) -> Tok {
        let t = self.refine(f);
        if t != self && t.is_param() {
            Tok(t.0 | ADDR)
        } else {
            t
        }
    }

    pub fn is_addr(self) -> bool {
        self.is_param() && self.0 & ADDR != 0
    }

    /// The value at the address this token is (a field address loads to the field's value).
    pub fn loaded(self) -> Tok {
        if self.is_addr() {
            Tok(self.0 & !ADDR)
        } else {
            self
        }
    }

    /// Without its field: the param or local itself.
    pub fn base(self) -> Tok {
        Tok(self.0 & !(ADDR | FIELD_MASK << 32))
    }
}

/// A sorted token set.
pub(super) type Set = Vec<Tok>;

pub(super) fn set_add(a: &mut Set, b: &[Tok]) {
    for &t in b {
        if let Err(i) = a.binary_search(&t) {
            a.insert(i, t);
        }
    }
}

pub(super) fn refine(s: &[Tok], f: u32) -> Set {
    let mut out = Vec::with_capacity(s.len());
    set_add(&mut out, &s.iter().map(|t| t.refine(f)).collect::<Vec<_>>());
    out
}

/// A value: what it carries as a whole, plus per field of a struct value (fields sorted).
#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct SVal {
    pub whole: Set,
    pub fields: Vec<(u32, Set)>,
}

impl SVal {
    pub fn of(whole: Set) -> SVal {
        SVal { whole, fields: Vec::new() }
    }

    pub fn tok(t: Tok) -> SVal {
        SVal::of(vec![t])
    }

    pub fn is_empty(&self) -> bool {
        self.whole.is_empty() && self.fields.iter().all(|(_, s)| s.is_empty())
    }

    /// Everything, fields included.
    pub fn flat(&self) -> Set {
        let mut out = self.whole.clone();
        for (_, s) in &self.fields {
            set_add(&mut out, s);
        }
        out
    }

    /// What reading field `f` of this value gives.
    pub fn select(&self, f: u32) -> Set {
        let mut out = refine(&self.whole, f);
        if let Ok(i) = self.fields.binary_search_by_key(&f, |(k, _)| *k) {
            set_add(&mut out, &self.fields[i].1);
        }
        out
    }

    pub fn add_field(&mut self, f: u32, s: &[Tok]) {
        if s.is_empty() {
            return;
        }
        match self.fields.binary_search_by_key(&f, |(k, _)| *k) {
            Ok(i) => set_add(&mut self.fields[i].1, s),
            Err(i) => self.fields.insert(i, (f, s.to_vec())),
        }
    }

    pub fn add(&mut self, o: &SVal) {
        set_add(&mut self.whole, &o.whole);
        for (f, s) in &o.fields {
            self.add_field(*f, s);
        }
    }
}
