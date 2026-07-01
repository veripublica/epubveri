//! Derivative-based RELAX NG validation (James Clark's algorithm), driven over
//! a `roxmltree` document. We compute the derivative of the start pattern with
//! respect to the XML event stream; the document is valid iff the final pattern
//! is `nullable`.
//!
//! Recursion: a [`Grammar`] keeps named definitions in `defs`, and `Pattern::Ref`
//! points into them *without inlining*, so recursive content models terminate
//! naturally (recursion in a valid RNG is always guarded by an `element`, whose
//! content is only expanded on the next start-tag event). `nullable` and
//! `startTagOpenDeriv` are **memoized at `Ref` boundaries** (the reused nodes),
//! which both bounds the work and guards against pathological unguarded cycles.
//!
//! Not yet done: hash-consing all patterns for cross-step memoization (needed to
//! tame the interleave-heavy XHTML content model at scale), and XSD facets.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::pattern::*;

/// A loaded schema: a start pattern plus the definitions `Ref` indexes into.
pub struct Grammar {
    pub start: Pat,
    pub defs: Vec<Pat>,
}

impl Grammar {
    /// A grammar with no named definitions (for API-built patterns).
    pub fn single(start: Pat) -> Self {
        Self {
            start,
            defs: Vec::new(),
        }
    }
}

fn is_ws(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// `applyAfter f p` — replace each `after(p1, k)` continuation `k` inside `p`
/// with `after(p1, f(k))`, distributing over choice. (No `Ref` appears here:
/// it only runs on `startTagOpenDeriv` results, which are after/choice/empty.)
fn apply_after<F: Fn(Pat) -> Pat>(p: &Pat, f: &F) -> Pat {
    match &**p {
        Pattern::After(p1, k) => after(p1.clone(), f(k.clone())),
        Pattern::Choice(a, b) => choice(apply_after(a, f), apply_after(b, f)),
        _ => not_allowed(),
    }
}

struct Env<'a> {
    defs: &'a [Pat],
    nullable_memo: RefCell<HashMap<usize, bool>>,
    nullable_busy: RefCell<HashSet<usize>>,
    open_memo: RefCell<HashMap<(usize, String, String), Pat>>,
    open_busy: RefCell<HashSet<usize>>,
}

impl<'a> Env<'a> {
    fn new(defs: &'a [Pat]) -> Self {
        Self {
            defs,
            nullable_memo: RefCell::new(HashMap::new()),
            nullable_busy: RefCell::new(HashSet::new()),
            open_memo: RefCell::new(HashMap::new()),
            open_busy: RefCell::new(HashSet::new()),
        }
    }

    fn nullable(&self, p: &Pat) -> bool {
        match &**p {
            Pattern::Empty | Pattern::Text => true,
            Pattern::Choice(a, b) => self.nullable(a) || self.nullable(b),
            Pattern::Group(a, b) | Pattern::Interleave(a, b) => {
                self.nullable(a) && self.nullable(b)
            }
            Pattern::OneOrMore(a) => self.nullable(a),
            Pattern::Ref(i) => {
                let i = *i;
                if let Some(v) = self.nullable_memo.borrow().get(&i) {
                    return *v;
                }
                if self.nullable_busy.borrow().contains(&i) {
                    return false; // unguarded cycle → least fixpoint
                }
                self.nullable_busy.borrow_mut().insert(i);
                let v = self.nullable(&self.defs[i]);
                self.nullable_busy.borrow_mut().remove(&i);
                self.nullable_memo.borrow_mut().insert(i, v);
                v
            }
            _ => false,
        }
    }

    fn text_deriv(&self, p: &Pat, s: &str) -> Pat {
        match &**p {
            Pattern::Choice(a, b) => choice(self.text_deriv(a, s), self.text_deriv(b, s)),
            Pattern::Interleave(a, b) => choice(
                interleave(self.text_deriv(a, s), b.clone()),
                interleave(a.clone(), self.text_deriv(b, s)),
            ),
            Pattern::Group(a, b) => {
                let p1 = group(self.text_deriv(a, s), b.clone());
                if self.nullable(a) {
                    choice(p1, self.text_deriv(b, s))
                } else {
                    p1
                }
            }
            Pattern::After(a, b) => after(self.text_deriv(a, s), b.clone()),
            Pattern::OneOrMore(a) => group(
                self.text_deriv(a, s),
                choice(one_or_more(a.clone()), empty()),
            ),
            Pattern::Text => text(),
            Pattern::Value(dt, v) => {
                if dt.equal(v, s) {
                    empty()
                } else {
                    not_allowed()
                }
            }
            Pattern::Data(dt) => {
                if dt.allows(s) {
                    empty()
                } else {
                    not_allowed()
                }
            }
            Pattern::List(inner) => {
                let mut cur = inner.clone();
                for tok in s.split_whitespace() {
                    cur = self.text_deriv(&cur, tok);
                }
                if self.nullable(&cur) {
                    empty()
                } else {
                    not_allowed()
                }
            }
            Pattern::Ref(i) => self.text_deriv(&self.defs[*i], s),
            _ => not_allowed(),
        }
    }

    fn start_tag_open_deriv(&self, p: &Pat, ns: &str, local: &str) -> Pat {
        match &**p {
            Pattern::Choice(a, b) => choice(
                self.start_tag_open_deriv(a, ns, local),
                self.start_tag_open_deriv(b, ns, local),
            ),
            Pattern::Element(nc, content) => {
                if nc.contains(ns, local) {
                    after(content.clone(), empty())
                } else {
                    not_allowed()
                }
            }
            Pattern::Interleave(a, b) => {
                let b2 = b.clone();
                let a2 = a.clone();
                choice(
                    apply_after(&self.start_tag_open_deriv(a, ns, local), &|k| {
                        interleave(k, b2.clone())
                    }),
                    apply_after(&self.start_tag_open_deriv(b, ns, local), &|k| {
                        interleave(a2.clone(), k)
                    }),
                )
            }
            Pattern::Group(a, b) => {
                let b2 = b.clone();
                let x = apply_after(&self.start_tag_open_deriv(a, ns, local), &|k| {
                    group(k, b2.clone())
                });
                if self.nullable(a) {
                    choice(x, self.start_tag_open_deriv(b, ns, local))
                } else {
                    x
                }
            }
            Pattern::OneOrMore(a) => {
                let a2 = a.clone();
                apply_after(&self.start_tag_open_deriv(a, ns, local), &|k| {
                    group(k, choice(one_or_more(a2.clone()), empty()))
                })
            }
            Pattern::After(a, b) => {
                let b2 = b.clone();
                apply_after(&self.start_tag_open_deriv(a, ns, local), &|k| {
                    after(k, b2.clone())
                })
            }
            Pattern::Ref(i) => {
                let i = *i;
                let key = (i, ns.to_string(), local.to_string());
                if let Some(p) = self.open_memo.borrow().get(&key) {
                    return p.clone();
                }
                if self.open_busy.borrow().contains(&i) {
                    return not_allowed();
                }
                self.open_busy.borrow_mut().insert(i);
                let r = self.start_tag_open_deriv(&self.defs[i], ns, local);
                self.open_busy.borrow_mut().remove(&i);
                self.open_memo.borrow_mut().insert(key, r.clone());
                r
            }
            _ => not_allowed(),
        }
    }

    fn value_match(&self, p: &Pat, s: &str) -> bool {
        (self.nullable(p) && is_ws(s)) || self.nullable(&self.text_deriv(p, s))
    }

    fn att_deriv(&self, p: &Pat, ns: &str, local: &str, val: &str) -> Pat {
        match &**p {
            Pattern::Choice(a, b) => choice(
                self.att_deriv(a, ns, local, val),
                self.att_deriv(b, ns, local, val),
            ),
            Pattern::Group(a, b) => choice(
                group(self.att_deriv(a, ns, local, val), b.clone()),
                group(a.clone(), self.att_deriv(b, ns, local, val)),
            ),
            Pattern::Interleave(a, b) => choice(
                interleave(self.att_deriv(a, ns, local, val), b.clone()),
                interleave(a.clone(), self.att_deriv(b, ns, local, val)),
            ),
            Pattern::After(a, b) => after(self.att_deriv(a, ns, local, val), b.clone()),
            Pattern::OneOrMore(a) => group(
                self.att_deriv(a, ns, local, val),
                choice(one_or_more(a.clone()), empty()),
            ),
            Pattern::Attribute(nc, content) => {
                if nc.contains(ns, local) && self.value_match(content, val) {
                    empty()
                } else {
                    not_allowed()
                }
            }
            Pattern::Ref(i) => self.att_deriv(&self.defs[*i], ns, local, val),
            _ => not_allowed(),
        }
    }

    fn start_tag_close_deriv(&self, p: &Pat) -> Pat {
        match &**p {
            Pattern::Choice(a, b) => {
                choice(self.start_tag_close_deriv(a), self.start_tag_close_deriv(b))
            }
            Pattern::Group(a, b) => {
                group(self.start_tag_close_deriv(a), self.start_tag_close_deriv(b))
            }
            Pattern::Interleave(a, b) => {
                interleave(self.start_tag_close_deriv(a), self.start_tag_close_deriv(b))
            }
            Pattern::OneOrMore(a) => one_or_more(self.start_tag_close_deriv(a)),
            Pattern::After(a, b) => after(self.start_tag_close_deriv(a), b.clone()),
            // an attribute that was never matched is now an error
            Pattern::Attribute(_, _) => not_allowed(),
            Pattern::Ref(i) => self.start_tag_close_deriv(&self.defs[*i]),
            _ => p.clone(),
        }
    }

    fn end_tag_deriv(&self, p: &Pat) -> Pat {
        match &**p {
            Pattern::Choice(a, b) => choice(self.end_tag_deriv(a), self.end_tag_deriv(b)),
            Pattern::After(a, b) => {
                if self.nullable(a) {
                    b.clone()
                } else {
                    not_allowed()
                }
            }
            Pattern::Ref(i) => self.end_tag_deriv(&self.defs[*i]),
            _ => not_allowed(),
        }
    }

    fn children_deriv(&self, p: &Pat, parent: roxmltree::Node) -> Pat {
        let mut cur = p.clone();
        for n in parent.children() {
            if n.is_element() {
                cur = self.child_deriv(&cur, n);
            } else if n.is_text() {
                let s = n.text().unwrap_or("");
                let d = self.text_deriv(&cur, s);
                cur = if is_ws(s) { choice(cur.clone(), d) } else { d };
            }
        }
        cur
    }

    fn child_deriv(&self, p: &Pat, node: roxmltree::Node) -> Pat {
        let ns = node.tag_name().namespace().unwrap_or("");
        let local = node.tag_name().name();
        let mut cur = self.start_tag_open_deriv(p, ns, local);
        for att in node.attributes() {
            let ans = att.namespace().unwrap_or("");
            cur = self.att_deriv(&cur, ans, att.name(), att.value());
        }
        cur = self.start_tag_close_deriv(&cur);
        cur = self.children_deriv(&cur, node);
        self.end_tag_deriv(&cur)
    }
}

/// Validate a root element node against `grammar`.
pub fn validate_node(grammar: &Grammar, root: roxmltree::Node) -> bool {
    let env = Env::new(&grammar.defs);
    let p = env.child_deriv(&grammar.start, root);
    env.nullable(&p)
}

/// Parse `xml` and validate its root element against `grammar`.
pub fn validate_xml(grammar: &Grammar, xml: &str) -> Result<bool, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    Ok(validate_node(grammar, doc.root_element()))
}
