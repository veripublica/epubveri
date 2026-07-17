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

    /// The distinct local names an element could legally start with at this
    /// point in the pattern - the pattern's first-set, restricted to names
    /// concrete enough to name in a message.
    ///
    /// Walked the same way as `nullable`: a `Group`/`After` contributes its
    /// second half only when its first half is nullable (an optional prefix
    /// lets a later element start), a `Choice` contributes both, and a `Ref`
    /// is followed once (a `visited` guard breaks recursive grammars). This
    /// is what turns "element X is not allowed here" into "…; expected one of
    /// li" - the set is collected at the point the offending element was
    /// rejected, so it is exactly what would have been accepted instead.
    fn expected_names(&self, p: &Pat, out: &mut Vec<String>, visited: &mut HashSet<usize>) {
        match &**p {
            Pattern::Element(nc, _) => {
                let mut locals = Vec::new();
                nc.concrete_locals(&mut locals);
                for l in locals {
                    if !out.iter().any(|e| e == l) {
                        out.push(l.to_string());
                    }
                }
            }
            Pattern::Choice(a, b) | Pattern::Interleave(a, b) => {
                self.expected_names(a, out, visited);
                self.expected_names(b, out, visited);
            }
            Pattern::Group(a, b) | Pattern::After(a, b) => {
                self.expected_names(a, out, visited);
                if self.nullable(a) {
                    self.expected_names(b, out, visited);
                }
            }
            Pattern::OneOrMore(a) => self.expected_names(a, out, visited),
            Pattern::Ref(i) => {
                if visited.insert(*i) {
                    self.expected_names(&self.defs[*i], out, visited);
                }
            }
            // Text/Data/Value/Attribute/Empty/NotAllowed name no element.
            _ => {}
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

    /// Whether an attribute of name `ns:local` is permitted anywhere in `p`,
    /// ignoring its value. Used only to classify an already-failed attribute:
    /// if the name is allowed here, the failure was the value, not the name.
    ///
    /// Walked like `expected_names` - through the compositors, following each
    /// `Ref` once under a visited-guard so a recursive grammar terminates.
    fn attr_name_allowed(
        &self,
        p: &Pat,
        ns: &str,
        local: &str,
        visited: &mut HashSet<usize>,
    ) -> bool {
        match &**p {
            Pattern::Attribute(nc, _) => nc.contains(ns, local),
            Pattern::Choice(a, b)
            | Pattern::Group(a, b)
            | Pattern::Interleave(a, b)
            | Pattern::After(a, b) => {
                self.attr_name_allowed(a, ns, local, visited)
                    || self.attr_name_allowed(b, ns, local, visited)
            }
            Pattern::OneOrMore(a) => self.attr_name_allowed(a, ns, local, visited),
            Pattern::Ref(i) => {
                visited.insert(*i) && self.attr_name_allowed(&self.defs[*i], ns, local, visited)
            }
            _ => false,
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
    /// Validate `parent`'s children against pattern `p`.
    ///
    /// `in_rejected` is set when descending *into* an element that was itself
    /// already rejected (for diagnostics - see the call site). In that mode
    /// loose text is ignored: the container is gone, so its text can't be
    /// "not allowed here" against a model it was never going to satisfy, and
    /// scoring it would double-report. Element children are still checked, so
    /// a bad element nested inside a bad container is still found.
    fn children_deriv<'d, 'i>(
        &self,
        p: &Pat,
        parent: roxmltree::Node<'d, 'i>,
        blames: &mut Vec<Blame<'d, 'i>>,
        in_rejected: bool,
    ) -> Pat {
        let mut cur = p.clone();
        for n in parent.children() {
            if n.is_element() {
                let ns = n.tag_name().namespace().unwrap_or("");
                let local = n.tag_name().name();
                if is_not_allowed(&self.start_tag_open_deriv(&cur, ns, local)) {
                    // Name not allowed here: report and skip it, keeping `cur`
                    // (an element that never fit can't have consumed anything),
                    // so the remaining siblings are still validated. `cur` is
                    // the pattern still expected at this position, so its
                    // first-set is exactly what would have been accepted.
                    let mut expected = Vec::new();
                    self.expected_names(&cur, &mut expected, &mut HashSet::new());
                    blames.push(Blame::Element(n, ElementFault::NotAllowed(expected)));
                    // Descend into the rejected element and check its children
                    // against this same position, for diagnostics only - the
                    // returned pattern is discarded so siblings still see the
                    // pre-rejection `cur`. A rejected container can hold errors
                    // of its own, and reporting only the container hides them:
                    // an obsolete `<center>` wrapping obsolete `<font>`/`<s>`
                    // would report just the `<center>` and stay silent on the
                    // rest, where epubcheck names each (issue #24).
                    let _ = self.children_deriv(&cur, n, blames, true);
                    continue;
                }
                cur = self.child_deriv(&cur, n, blames);
                if is_not_allowed(&cur) {
                    break; // the child's own attributes/content couldn't recover
                }
            } else if n.is_text() {
                if in_rejected {
                    continue;
                }
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
                        blames.push(Blame::Element(parent, ElementFault::TextNotAllowed));
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
            let mut expected = Vec::new();
            self.expected_names(p, &mut expected, &mut HashSet::new());
            blames.push(Blame::Element(node, ElementFault::NotAllowed(expected)));
            return cur;
        }
        for att in node.attributes() {
            let ans = att.namespace().unwrap_or("");
            let prev = cur.clone();
            cur = self.att_deriv(&cur, ans, att.name(), att.value());
            if is_not_allowed(&cur) {
                // *This* attribute is the culprit — pin it, so the finding can
                // target `@name`. Distinguish the two ways it can fail: an
                // attribute of this name isn't allowed at all, vs. the name is
                // allowed but its value doesn't satisfy the datatype. Only the
                // first is "not allowed here"; the second is a value error, and
                // the value is worth quoting. `att_deriv` collapses both into
                // NotAllowed, so re-ask ignoring the value to tell them apart.
                let fault = if self.attr_name_allowed(&prev, ans, att.name(), &mut HashSet::new()) {
                    AttributeFault::InvalidValue
                } else {
                    AttributeFault::NotAllowed
                };
                blames.push(Blame::Attribute(node, att, fault));
                return cur;
            }
        }
        cur = self.start_tag_close_deriv(&cur);
        if is_not_allowed(&cur) {
            blames.push(Blame::Element(node, ElementFault::MissingAttribute));
            return cur;
        }
        cur = self.children_deriv(&cur, node, blames, false);
        if is_not_allowed(&cur) {
            return cur; // a descendant failed; `children_deriv` recorded it
        }
        cur = self.end_tag_deriv(&cur);
        if is_not_allowed(&cur) {
            blames.push(Blame::Element(node, ElementFault::IncompleteContent));
        }
        cur
    }
}

fn is_not_allowed(p: &Pat) -> bool {
    matches!(**p, Pattern::NotAllowed)
}

/// Which way an element broke the content model - the four distinct faults an
/// [`Blame::Element`] can stand for. Kept separate because they need different
/// wording: an element that is simply misplaced is *not* the same as one whose
/// content or attributes are the problem, and reporting all of them as "not
/// allowed here" would be plainly wrong for three of the four cases.
/// Which way an attribute broke the content model. An attribute *name* that
/// isn't permitted here and a permitted attribute with an invalid *value* are
/// different problems and need different wording - "not allowed here" is
/// wrong for the second, which is a real value we can quote.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttributeFault {
    /// No attribute of this name is allowed at this position.
    NotAllowed,
    /// The name is allowed, but the value doesn't satisfy its datatype.
    InvalidValue,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ElementFault {
    /// The element itself is not permitted at this position. Carries the
    /// local names that *would* have been accepted there (the pattern's
    /// first-set at the point of rejection), for the "; expected one of …"
    /// tail - empty when the position admits only wildcard-named content, in
    /// which case no suggestion is made.
    NotAllowed(Vec<String>),
    /// Non-whitespace character data is not permitted directly in this element.
    TextNotAllowed,
    /// The element is missing a required attribute.
    MissingAttribute,
    /// The element's content is incomplete - a required child is absent.
    IncompleteContent,
}

/// Where content-model validation failed (issues #17/#18): an element (tagged
/// with *how* it failed, see [`ElementFault`]), or a specific present attribute
/// of one. The [`Attribute`](Blame::Attribute) case lets the diagnostic pin
/// `@name` (with a real position + element path) when the violation is
/// attribute-level (e.g. `attribute "role" is not allowed here`), rather than
/// only naming the containing element.
pub enum Blame<'d, 'i> {
    Element(roxmltree::Node<'d, 'i>, ElementFault),
    Attribute(
        roxmltree::Node<'d, 'i>,
        roxmltree::Attribute<'d, 'i>,
        AttributeFault,
    ),
}

impl<'d, 'i> Blame<'d, 'i> {
    /// The element whose source position anchors this finding (the containing
    /// element for an attribute-level blame).
    pub fn node(&self) -> roxmltree::Node<'d, 'i> {
        match self {
            Blame::Element(n, _) | Blame::Attribute(n, ..) => *n,
        }
    }

    /// The specific attribute this finding pins, when attribute-level.
    pub fn attribute(&self) -> Option<roxmltree::Attribute<'d, 'i>> {
        match self {
            Blame::Attribute(_, a, _) => Some(*a),
            Blame::Element(..) => None,
        }
    }

    /// A human-readable description of the fault, naming the offending element
    /// or attribute, plus the same name as a structured `params` entry (for
    /// i18n / machine consumers). Replaces the old blanket "does not conform to
    /// the … schema" wording with something that says *what* is wrong, in the
    /// style of epubcheck's own RSC-005 messages.
    pub fn describe(&self) -> (String, Vec<String>) {
        match self {
            Blame::Element(n, fault) => {
                let name = n.tag_name().name();
                let mut params = vec![name.to_string()];
                let text = match fault {
                    ElementFault::NotAllowed(expected) => {
                        let mut t = format!("element \"{name}\" is not allowed here");
                        // Name what would have fit - but only when the set is
                        // small enough to be a real constraint. Our grammar is
                        // deliberately permissive on nesting order (flow and
                        // phrasing share one large pool), so a loose position's
                        // first-set can run to 80+ names; that isn't "expected
                        // X", it's "almost anything is allowed here", and
                        // printing it would bury the actual problem. A tight
                        // model (an `<html>` wanting `head`, a table row wanting
                        // cells) yields a handful, which is exactly the case
                        // worth naming. epubcheck draws the same practical line.
                        // Threshold high enough to print a genuine content
                        // model (XHTML 1.1's body-level block set is ~22 names,
                        // which epubcheck lists in full), low enough to suppress
                        // our permissive pools' 80-name flow set, which is
                        // "almost anything" rather than a constraint.
                        const MAX_SUGGESTED: usize = 24;
                        let distinct = distinct_sorted(expected);
                        if !distinct.is_empty() && distinct.len() <= MAX_SUGGESTED {
                            t.push_str(&format!("; expected {}", one_of(&distinct)));
                            params.extend(distinct);
                        }
                        t
                    }
                    ElementFault::TextNotAllowed => {
                        format!("character data is not allowed in element \"{name}\"")
                    }
                    ElementFault::MissingAttribute => {
                        format!("element \"{name}\" is missing a required attribute")
                    }
                    ElementFault::IncompleteContent => {
                        format!("element \"{name}\" has incomplete content")
                    }
                };
                (text, params)
            }
            Blame::Attribute(_, a, fault) => {
                let name = a.name();
                match fault {
                    AttributeFault::NotAllowed => (
                        format!("attribute \"{name}\" is not allowed here"),
                        vec![name.to_string()],
                    ),
                    // The name is fine; the value is the problem, so quote it -
                    // "not allowed here" would send the author looking at the
                    // wrong thing. Params carry the name then the value.
                    AttributeFault::InvalidValue => (
                        format!(
                            "value of attribute \"{name}\" is invalid: \"{}\"",
                            a.value()
                        ),
                        vec![name.to_string(), a.value().to_string()],
                    ),
                }
            }
        }
    }
}

/// The first-set as a stable, de-duplicated, sorted list - so both the count
/// (for the suggestion threshold) and the message order are deterministic
/// regardless of the order names fell out of the pattern.
fn distinct_sorted(names: &[String]) -> Vec<String> {
    let mut v: Vec<String> = names.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

/// One name reads as `"head"`; several as `one of "td", "th"`.
pub(crate) fn one_of(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    match quoted.as_slice() {
        [only] => only.clone(),
        _ => format!("one of {}", quoted.join(", ")),
    }
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
        blames.push(Blame::Element(root, ElementFault::IncompleteContent));
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
