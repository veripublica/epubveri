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

    // The two tree-walking derivatives collect a `blames` list so a failed
    // validation can name *which* nodes broke the content model (issues
    // #17/#18), and — with lightweight error recovery — *all* of them, not just
    // the first (matching epubcheck, which lists every offending node). The
    // recovery is deliberately bounded: when a child element's *name* isn't
    // allowed at its position, it's reported and skipped (the rest of the
    // siblings are still checked); a child whose own attributes or content can't
    // be validated is reported and then halts that branch. A document is valid
    // iff `blames` stays empty — recovery may leave the final pattern nullable
    // even when errors were found, so nullability is no longer the oracle.
    fn children_deriv<'d, 'i>(
        &self,
        p: &Pat,
        parent: roxmltree::Node<'d, 'i>,
        blames: &mut Vec<Blame<'d, 'i>>,
    ) -> Pat {
        let mut cur = p.clone();
        for n in parent.children() {
            if n.is_element() {
                let ns = n.tag_name().namespace().unwrap_or("");
                let local = n.tag_name().name();
                if is_not_allowed(&self.start_tag_open_deriv(&cur, ns, local)) {
                    // Name not allowed here: report and skip it, keeping `cur`
                    // (an element that never fit can't have consumed anything),
                    // so the remaining siblings are still validated.
                    blames.push(Blame::Element(n));
                    continue;
                }
                cur = self.child_deriv(&cur, n, blames);
                if is_not_allowed(&cur) {
                    break; // the child's own attributes/content couldn't recover
                }
            } else if n.is_text() {
                let s = n.text().unwrap_or("");
                if is_ws(s) {
                    // Whitespace is harmless: `choice(cur, NA) = cur`, so an
                    // ignorable run never disturbs the pattern.
                    cur = choice(cur.clone(), self.text_deriv(&cur, s));
                } else {
                    let d = self.text_deriv(&cur, s);
                    if is_not_allowed(&d) {
                        // Loose text not allowed: report (at the containing
                        // element) and skip, keeping `cur`.
                        blames.push(Blame::Element(parent));
                    } else {
                        cur = d;
                    }
                }
            }
        }
        cur
    }

    fn child_deriv<'d, 'i>(
        &self,
        p: &Pat,
        node: roxmltree::Node<'d, 'i>,
        blames: &mut Vec<Blame<'d, 'i>>,
    ) -> Pat {
        let ns = node.tag_name().namespace().unwrap_or("");
        let local = node.tag_name().name();
        let mut cur = self.start_tag_open_deriv(p, ns, local);
        if is_not_allowed(&cur) {
            // Only reached for the root element; sibling name-mismatches are
            // handled (and skipped) in `children_deriv`.
            blames.push(Blame::Element(node));
            return cur;
        }
        for att in node.attributes() {
            let ans = att.namespace().unwrap_or("");
            cur = self.att_deriv(&cur, ans, att.name(), att.value());
            if is_not_allowed(&cur) {
                // *This* attribute (present, but not allowed / invalid value) is
                // the culprit — pin it, so the finding can target `@name`.
                blames.push(Blame::Attribute(node, att));
                return cur;
            }
        }
        cur = self.start_tag_close_deriv(&cur);
        if is_not_allowed(&cur) {
            blames.push(Blame::Element(node)); // a required attribute is missing
            return cur;
        }
        cur = self.children_deriv(&cur, node, blames);
        if is_not_allowed(&cur) {
            return cur; // a descendant failed; `children_deriv` recorded it
        }
        cur = self.end_tag_deriv(&cur);
        if is_not_allowed(&cur) {
            blames.push(Blame::Element(node)); // this element's content is incomplete
        }
        cur
    }
}

fn is_not_allowed(p: &Pat) -> bool {
    matches!(**p, Pattern::NotAllowed)
}

/// Where content-model validation failed (issues #17/#18): an element, or a
/// specific present attribute of one. The [`Attribute`](Blame::Attribute) case
/// lets the diagnostic pin `@name` (with a real position + element path) when
/// the violation is attribute-level (e.g. `attribute "opf:role" not allowed
/// here`), rather than only naming the containing element.
pub enum Blame<'d, 'i> {
    Element(roxmltree::Node<'d, 'i>),
    Attribute(roxmltree::Node<'d, 'i>, roxmltree::Attribute<'d, 'i>),
}

/// Validate a root element node against `grammar`, returning every node that
/// broke the content model ([`Blame`]) in document order — empty if the document
/// is valid. Each blame gives a real `line:column` and element path for its
/// diagnostic, instead of anchoring the whole document at its root and reporting
/// only the first problem (issues #17/#18).
pub fn validate_node_report<'d, 'i>(
    grammar: &Grammar,
    root: roxmltree::Node<'d, 'i>,
) -> Vec<Blame<'d, 'i>> {
    let env = Env::new(&grammar.defs);
    let mut blames = Vec::new();
    let p = env.child_deriv(&grammar.start, root, &mut blames);
    // Validity is "no blames": recovery may leave `p` nullable even after
    // finding errors. The nullability check here is only a defensive net — if a
    // pattern somehow ends non-nullable with nothing recorded, still surface the
    // document as invalid rather than silently pass it.
    if !env.nullable(&p) && blames.is_empty() {
        blames.push(Blame::Element(root));
    }
    blames
}

/// Validate a root element node against `grammar` (valid iff nothing to blame).
pub fn validate_node(grammar: &Grammar, root: roxmltree::Node) -> bool {
    validate_node_report(grammar, root).is_empty()
}

/// Parse `xml` and validate its root element against `grammar`.
pub fn validate_xml(grammar: &Grammar, xml: &str) -> Result<bool, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    Ok(validate_node(grammar, doc.root_element()))
}
