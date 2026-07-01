//! RELAX NG patterns (simplified syntax) + name classes + datatypes, with
//! "smart constructors" that keep patterns in a small normal form (collapsing
//! `notAllowed`/`empty`). This is the data model for the derivative-based
//! validation algorithm in `derive.rs` (James Clark, "An algorithm for RELAX
//! NG validation").

use std::rc::Rc;

pub type Pat = Rc<Pattern>;

pub use super::datatype::Datatype;

#[derive(Debug, Clone)]
pub enum NameClass {
    AnyName,
    AnyNameExcept(Box<NameClass>),
    Name { ns: String, local: String },
    NsName { ns: String },
    NsNameExcept { ns: String, except: Box<NameClass> },
    Choice(Box<NameClass>, Box<NameClass>),
}

impl NameClass {
    pub fn contains(&self, ns: &str, local: &str) -> bool {
        match self {
            NameClass::AnyName => true,
            NameClass::AnyNameExcept(e) => !e.contains(ns, local),
            NameClass::Name { ns: n, local: l } => n == ns && l == local,
            NameClass::NsName { ns: n } => n == ns,
            NameClass::NsNameExcept { ns: n, except } => n == ns && !except.contains(ns, local),
            NameClass::Choice(a, b) => a.contains(ns, local) || b.contains(ns, local),
        }
    }
}

#[derive(Debug)]
pub enum Pattern {
    Empty,
    NotAllowed,
    Text,
    Choice(Pat, Pat),
    Interleave(Pat, Pat),
    Group(Pat, Pat),
    OneOrMore(Pat),
    /// Internal continuation produced while validating: `after(p1, p2)` means
    /// "still matching content `p1`; once it ends, continue with `p2`".
    After(Pat, Pat),
    Element(NameClass, Pat),
    Attribute(NameClass, Pat),
    Data(Datatype),
    Value(Datatype, String),
    List(Pat),
    /// A reference to a named definition, by index into `Grammar::defs`.
    /// Kept un-expanded so recursive grammars are representable.
    Ref(usize),
}

fn is_na(p: &Pat) -> bool {
    matches!(**p, Pattern::NotAllowed)
}
fn is_empty(p: &Pat) -> bool {
    matches!(**p, Pattern::Empty)
}

// --- smart constructors ---

pub fn empty() -> Pat {
    Rc::new(Pattern::Empty)
}
pub fn not_allowed() -> Pat {
    Rc::new(Pattern::NotAllowed)
}
pub fn text() -> Pat {
    Rc::new(Pattern::Text)
}

pub fn choice(a: Pat, b: Pat) -> Pat {
    if is_na(&a) {
        b
    } else if is_na(&b) {
        a
    } else {
        Rc::new(Pattern::Choice(a, b))
    }
}

pub fn group(a: Pat, b: Pat) -> Pat {
    if is_na(&a) || is_na(&b) {
        not_allowed()
    } else if is_empty(&a) {
        b
    } else if is_empty(&b) {
        a
    } else {
        Rc::new(Pattern::Group(a, b))
    }
}

pub fn interleave(a: Pat, b: Pat) -> Pat {
    if is_na(&a) || is_na(&b) {
        not_allowed()
    } else if is_empty(&a) {
        b
    } else if is_empty(&b) {
        a
    } else {
        Rc::new(Pattern::Interleave(a, b))
    }
}

pub fn after(a: Pat, b: Pat) -> Pat {
    if is_na(&a) || is_na(&b) {
        not_allowed()
    } else {
        Rc::new(Pattern::After(a, b))
    }
}

pub fn one_or_more(a: Pat) -> Pat {
    if is_na(&a) {
        not_allowed()
    } else {
        Rc::new(Pattern::OneOrMore(a))
    }
}

pub fn element(nc: NameClass, content: Pat) -> Pat {
    Rc::new(Pattern::Element(nc, content))
}
pub fn attribute(nc: NameClass, content: Pat) -> Pat {
    Rc::new(Pattern::Attribute(nc, content))
}
pub fn data(dt: Datatype) -> Pat {
    Rc::new(Pattern::Data(dt))
}
pub fn value(dt: Datatype, v: impl Into<String>) -> Pat {
    Rc::new(Pattern::Value(dt, v.into()))
}
pub fn list(p: Pat) -> Pat {
    if is_na(&p) {
        not_allowed()
    } else {
        Rc::new(Pattern::List(p))
    }
}
pub fn ref_(index: usize) -> Pat {
    Rc::new(Pattern::Ref(index))
}

pub fn optional(p: Pat) -> Pat {
    choice(p, empty())
}
pub fn zero_or_more(p: Pat) -> Pat {
    choice(one_or_more(p), empty())
}

/// `nullable` lives in `derive.rs` now (it must resolve `Ref` against the
/// grammar's definitions), so it is not a free function here.

/// A `<name ns:local>` name class with no namespace (the common attribute case).
pub fn local_name(local: &str) -> NameClass {
    NameClass::Name {
        ns: String::new(),
        local: local.into(),
    }
}
/// A `<name>` name class in the given namespace.
pub fn qname(ns: &str, local: &str) -> NameClass {
    NameClass::Name {
        ns: ns.into(),
        local: local.into(),
    }
}
