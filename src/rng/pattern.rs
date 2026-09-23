//! RELAX NG patterns (simplified syntax) + name classes + datatypes, with
//! "smart constructors" that keep patterns in a small normal form (collapsing
//! `notAllowed`/`empty`). This is the data model for the derivative-based
//! validation algorithm in `derive.rs` (James Clark, "An algorithm for RELAX
//! NG validation").
//!
//! **Hash-consing.** Every `Pattern` is interned (see `intern` below), so
//! structurally-identical patterns always share one `Rc` allocation and
//! `Rc::ptr_eq` becomes a reliable, O(1) "are these the same state" check.
//! `choice()` uses this to collapse `choice(a, a) -> a`, which is what makes
//! whitespace-derivative steps against `interleave`/`mixed` patterns collapse
//! back to a no-op instead of doubling the pattern tree at every whitespace
//! text node — without this, `<mixed>`-heavy content models (ordinary flowing
//! prose) blow up exponentially after only ~15-20 sibling elements.

use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::rc::Rc;

/// A fast, non-cryptographic hasher for the engine's own tables: the
/// interning set and the derivative memos in `derive.rs`.
///
/// Their keys are pattern addresses, small integers and strings that come
/// from the embedded schemas, never from the book being validated, so the
/// flooding resistance std's SipHash pays for buys nothing here, and it was a
/// visible share of validation time. This is the rotate-xor-multiply scheme
/// rustc uses for its own tables.
#[derive(Default, Clone, Copy)]
pub(crate) struct FastHasher(u64);

impl FastHasher {
    const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::K);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for w in words {
            self.add(u64::from_le_bytes(*w));
        }
        if !rest.is_empty() {
            let mut last = [0u8; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// `BuildHasher` for [`FastHasher`].
pub(crate) type FastHash = BuildHasherDefault<FastHasher>;

pub type Pat = Rc<Pattern>;

pub use super::datatype::Datatype;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

    /// The concrete local names this class names outright, appended to `out`.
    ///
    /// Only `Name` classes contribute a name a reader could act on. The
    /// wildcards (`AnyName`, `NsName`, and the `*Except` forms) match by rule
    /// rather than by enumerating members, so there is no finite list to
    /// suggest - they are deliberately skipped, which is why the caller must
    /// treat an empty result as "can't say" rather than "nothing is allowed".
    pub fn concrete_locals<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            NameClass::Name { local, .. } => out.push(local),
            NameClass::Choice(a, b) => {
                a.concrete_locals(out);
                b.concrete_locals(out);
            }
            NameClass::AnyName
            | NameClass::AnyNameExcept(_)
            | NameClass::NsName { .. }
            | NameClass::NsNameExcept { .. } => {}
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

// Manual (not derived) `PartialEq`/`Hash`: any `Pat` (`Rc<Pattern>`) field is
// compared/hashed by *pointer identity*, never by recursing into the
// pointee. This is the hash-consing trick — since children are always
// interned before their parent is constructed, pointer equality of children
// already implies deep structural equality, far more cheaply than recursing.
impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        use Pattern::*;
        match (self, other) {
            (Empty, Empty) | (NotAllowed, NotAllowed) | (Text, Text) => true,
            (Choice(a1, b1), Choice(a2, b2))
            | (Interleave(a1, b1), Interleave(a2, b2))
            | (Group(a1, b1), Group(a2, b2))
            | (After(a1, b1), After(a2, b2)) => Rc::ptr_eq(a1, a2) && Rc::ptr_eq(b1, b2),
            (OneOrMore(a1), OneOrMore(a2)) | (List(a1), List(a2)) => Rc::ptr_eq(a1, a2),
            (Element(n1, c1), Element(n2, c2)) | (Attribute(n1, c1), Attribute(n2, c2)) => {
                n1 == n2 && Rc::ptr_eq(c1, c2)
            }
            (Data(d1), Data(d2)) => d1 == d2,
            (Value(d1, v1), Value(d2, v2)) => d1 == d2 && v1 == v2,
            (Ref(i1), Ref(i2)) => i1 == i2,
            _ => false,
        }
    }
}
impl Eq for Pattern {}

impl Hash for Pattern {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Pattern::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Empty | NotAllowed | Text => {}
            Choice(a, b) | Interleave(a, b) | Group(a, b) | After(a, b) => {
                (Rc::as_ptr(a) as usize).hash(state);
                (Rc::as_ptr(b) as usize).hash(state);
            }
            OneOrMore(a) | List(a) => (Rc::as_ptr(a) as usize).hash(state),
            Element(n, c) | Attribute(n, c) => {
                n.hash(state);
                (Rc::as_ptr(c) as usize).hash(state);
            }
            Data(d) => d.hash(state),
            Value(d, v) => {
                d.hash(state);
                v.hash(state);
            }
            Ref(i) => i.hash(state),
        }
    }
}

thread_local! {
    static INTERN: RefCell<HashSet<Pat, FastHash>> = RefCell::new(HashSet::default());
    // The three leaves, built once per thread. They were interned on every
    // call, and `not_allowed()` alone is called for every branch a derivative
    // rules out.
    static EMPTY: Pat = intern(Pattern::Empty);
    static NOT_ALLOWED: Pat = intern(Pattern::NotAllowed);
    static TEXT: Pat = intern(Pattern::Text);
}

/// Intern a freshly-built `Pattern`: if a structurally-identical pattern was
/// already constructed, return the existing (canonical) `Rc`; otherwise
/// register this one as canonical. `HashSet<Pat>` hashes/compares through
/// `Pattern`'s manual impls above via `Rc<T>`'s standard delegating
/// `Hash`/`Eq` impls, so no wrapper type is needed.
///
/// Looked up by value first, so a pattern that already exists, which is most
/// of them, costs no allocation: `Rc<Pattern>: Borrow<Pattern>`, and `Rc`'s
/// `Hash`/`Eq` delegate, so the set can be asked with the bare `Pattern`.
fn intern(p: Pattern) -> Pat {
    INTERN.with(|cell| {
        let mut set = cell.borrow_mut();
        if let Some(existing) = set.get(&p) {
            return existing.clone();
        }
        let rc = Rc::new(p);
        set.insert(rc.clone());
        rc
    })
}

/// Clear the pattern-interning cache. In a long-lived embedded process
/// (validating many books), the cache otherwise grows for the life of the
/// process; calling this between independent top-level validation runs (see
/// `validate_bytes` in `src/lib.rs`) bounds memory to roughly one book's
/// working set instead. A known, accepted trade-off for now, not a
/// correctness concern — revisit with real profiling data if it matters.
pub fn clear_intern_cache() {
    INTERN.with(|cell| cell.borrow_mut().clear());
}

fn is_na(p: &Pat) -> bool {
    matches!(**p, Pattern::NotAllowed)
}
fn is_empty(p: &Pat) -> bool {
    matches!(**p, Pattern::Empty)
}

// --- smart constructors ---

pub fn empty() -> Pat {
    EMPTY.with(Rc::clone)
}
pub fn not_allowed() -> Pat {
    NOT_ALLOWED.with(Rc::clone)
}
pub fn text() -> Pat {
    TEXT.with(Rc::clone)
}

// `is_na(&a) -> b` and `is_na(&b) -> a` return different values; clippy sees
// two arms that look alike because each returns "the other one". Collapsing
// them would mean writing the condition as a disjunction and then picking a
// side again inside it, which is longer and says less.
#[allow(clippy::if_same_then_else)]
pub fn choice(a: Pat, b: Pat) -> Pat {
    if is_na(&a) {
        b
    } else if is_na(&b) {
        a
    } else if Rc::ptr_eq(&a, &b) {
        a
    } else {
        intern(Pattern::Choice(a, b))
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
        intern(Pattern::Group(a, b))
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
        intern(Pattern::Interleave(a, b))
    }
}

pub fn after(a: Pat, b: Pat) -> Pat {
    if is_na(&a) || is_na(&b) {
        not_allowed()
    } else {
        intern(Pattern::After(a, b))
    }
}

pub fn one_or_more(a: Pat) -> Pat {
    if is_na(&a) {
        not_allowed()
    } else {
        intern(Pattern::OneOrMore(a))
    }
}

pub fn element(nc: NameClass, content: Pat) -> Pat {
    intern(Pattern::Element(nc, content))
}
pub fn attribute(nc: NameClass, content: Pat) -> Pat {
    intern(Pattern::Attribute(nc, content))
}
pub fn data(dt: Datatype) -> Pat {
    intern(Pattern::Data(dt))
}
pub fn value(dt: Datatype, v: impl Into<String>) -> Pat {
    intern(Pattern::Value(dt, v.into()))
}
pub fn list(p: Pat) -> Pat {
    if is_na(&p) {
        not_allowed()
    } else {
        intern(Pattern::List(p))
    }
}
pub fn ref_(index: usize) -> Pat {
    intern(Pattern::Ref(index))
}

pub fn optional(p: Pat) -> Pat {
    choice(p, empty())
}
pub fn zero_or_more(p: Pat) -> Pat {
    choice(one_or_more(p), empty())
}

// `nullable` lives in `derive.rs` now (it must resolve `Ref` against the
// grammar's definitions), so it is not a free function here.

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
