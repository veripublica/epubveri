//! Evaluator for the AST in `ast.rs`, against a `roxmltree` document.
//!
//! **A real XPath 1.0 gotcha this gets right on purpose:** an unprefixed
//! name in a node-test (`head`, `@id`) means the **null namespace** in
//! XPath, never "whatever the instance document's default namespace is" —
//! unlike normal XML name resolution. Since OPF/XHTML documents almost
//! always use a default namespace, Schematron rules (ours and epubcheck's
//! real ones) always use an explicit prefix bound via the schema's own
//! `<ns>` declarations (`opf:package`, `h:head`, ...), resolved here via the
//! `namespaces` table on `EvalCtx` — never against the instance document's
//! own namespace bindings.
//!
//! Deliberately excluded (see this project's `CLAUDE.md` / the plan this
//! increment shipped under): `matches()`/`tokenize()` (regex),
//! `resolve-uri()`. Calling an unknown function evaluates to an empty
//! node-set/false rather than erroring — consistent with this project's
//! "never hard-fail" posture for schema-shaped checks.

use std::collections::HashMap;

use roxmltree::Node;

use super::ast::{Axis, BinOp, Expr, NameTest, Path, PathStart};

#[derive(Debug, Clone)]
pub enum NodeRef<'a, 'input> {
    Elem(Node<'a, 'input>),
    Attr {
        owner: Node<'a, 'input>,
        ns: Option<String>,
        local: String,
        value: String,
    },
}

impl<'a, 'input> NodeRef<'a, 'input> {
    pub fn string_value(&self) -> String {
        match self {
            NodeRef::Elem(n) => element_string_value(*n),
            NodeRef::Attr { value, .. } => value.clone(),
        }
    }

    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (NodeRef::Elem(a), NodeRef::Elem(b)) => a.id() == b.id(),
            (
                NodeRef::Attr {
                    owner: o1,
                    local: l1,
                    ns: n1,
                    ..
                },
                NodeRef::Attr {
                    owner: o2,
                    local: l2,
                    ns: n2,
                    ..
                },
            ) => o1.id() == o2.id() && l1 == l2 && n1 == n2,
            _ => false,
        }
    }
}

fn element_string_value(n: Node) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        if d.is_text()
            && let Some(t) = d.text()
        {
            s.push_str(t);
        }
    }
    s
}

fn dedup(nodes: &mut Vec<NodeRef>) {
    let mut out: Vec<NodeRef> = Vec::with_capacity(nodes.len());
    for n in nodes.drain(..) {
        if !out.iter().any(|o| o.same_as(&n)) {
            out.push(n);
        }
    }
    *nodes = out;
}

#[derive(Debug, Clone)]
pub enum Value<'a, 'input> {
    NodeSet(Vec<NodeRef<'a, 'input>>),
    String(String),
    Number(f64),
    Boolean(bool),
}

impl<'a, 'input> Value<'a, 'input> {
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::NodeSet(ns) => !ns.is_empty(),
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::Boolean(b) => *b,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            Value::NodeSet(ns) => ns
                .first()
                .map(|n| n.string_value())
                .unwrap_or_default()
                .trim()
                .parse()
                .unwrap_or(f64::NAN),
            Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
            Value::Number(n) => *n,
            Value::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub fn to_string_value(&self) -> String {
        match self {
            Value::NodeSet(ns) => ns.first().map(|n| n.string_value()).unwrap_or_default(),
            Value::String(s) => s.clone(),
            Value::Number(n) => format_number(*n),
            Value::Boolean(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
        }
    }

    fn into_nodeset(self) -> Vec<NodeRef<'a, 'input>> {
        match self {
            Value::NodeSet(ns) => ns,
            _ => Vec::new(),
        }
    }
}

fn format_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n == 0.0 {
        "0".to_string()
    } else if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Evaluation context threaded through a single top-level expression
/// evaluation. `current` is the rule's original context node (what
/// `current()` returns) — fixed for the whole evaluation, unlike the
/// navigational "current node-set" that predicates shift.
pub struct Env<'a, 'input, 'e> {
    pub root: Node<'a, 'input>,
    pub current: NodeRef<'a, 'input>,
    pub vars: &'e HashMap<String, Value<'a, 'input>>,
    pub namespaces: &'e HashMap<String, String>,
}

/// Evaluate `expr` as a boolean, with `context` as the single context node
/// (the typical Schematron `assert`/`report` `test="..."` entry point).
pub fn eval_boolean<'a, 'input>(
    expr: &Expr,
    env: &Env<'a, 'input, '_>,
    context: Node<'a, 'input>,
) -> bool {
    eval(expr, env, &[NodeRef::Elem(context)]).to_boolean()
}

pub fn eval<'a, 'input>(
    expr: &Expr,
    env: &Env<'a, 'input, '_>,
    nodes: &[NodeRef<'a, 'input>],
) -> Value<'a, 'input> {
    match expr {
        Expr::Number(n) => Value::Number(*n),
        Expr::Str(s) => Value::String(s.clone()),
        Expr::Variable(name) => env
            .vars
            .get(name)
            .cloned()
            .unwrap_or(Value::NodeSet(Vec::new())),
        Expr::Neg(e) => Value::Number(-eval(e, env, nodes).to_number()),
        Expr::Call(name, args) => eval_call(name, args, env, nodes),
        Expr::Union(a, b) => {
            let mut ns = eval(a, env, nodes).into_nodeset();
            ns.extend(eval(b, env, nodes).into_nodeset());
            dedup(&mut ns);
            Value::NodeSet(ns)
        }
        Expr::BinOp(a, op, b) => eval_binop(a, *op, b, env, nodes),
        Expr::Path(p) => eval_path(p, env, nodes),
    }
}

fn eval_path<'a, 'input>(
    path: &Path,
    env: &Env<'a, 'input, '_>,
    nodes: &[NodeRef<'a, 'input>],
) -> Value<'a, 'input> {
    let mut steps = path.steps.iter();
    let mut current: Vec<NodeRef<'a, 'input>> = match &path.start {
        // XPath's "/" navigates from the *document-node*, whose only child
        // is the root element. Only the `Child` axis is actually affected
        // by this: `/opf:package`'s first step must test the root element
        // itself directly (never found by searching root's own children,
        // which is what a normal Child-axis step does). Every *other*
        // first-step axis — crucially `DescendantOrSelf`, which is what
        // `//` desugars its leading step to — behaves exactly as it would
        // on any ordinary node (self + descendants), so it reuses the
        // normal per-step machinery unchanged.
        PathStart::Root => match steps.next() {
            None => return Value::NodeSet(vec![NodeRef::Elem(env.root)]),
            Some(first) => {
                let mut candidates = if first.axis == Axis::Child {
                    if name_test_matches_elem(env.root, &first.test, env.namespaces) {
                        vec![NodeRef::Elem(env.root)]
                    } else {
                        Vec::new()
                    }
                } else {
                    let mut v = axis_step(
                        &NodeRef::Elem(env.root),
                        first.axis,
                        &first.test,
                        env.namespaces,
                    );
                    dedup(&mut v);
                    v
                };
                candidates.retain(|cand| {
                    first
                        .predicates
                        .iter()
                        .all(|p| eval(p, env, std::slice::from_ref(cand)).to_boolean())
                });
                candidates
            }
        },
        PathStart::Relative => nodes.to_vec(),
        PathStart::Expr(e) => {
            let v = eval(e, env, nodes);
            if path.steps.is_empty() {
                return v;
            }
            v.into_nodeset()
        }
    };
    for step in steps {
        let mut next = Vec::new();
        for n in &current {
            next.extend(axis_step(n, step.axis, &step.test, env.namespaces));
        }
        dedup(&mut next);
        current = next
            .into_iter()
            .filter(|cand| {
                step.predicates
                    .iter()
                    .all(|pred| eval(pred, env, std::slice::from_ref(cand)).to_boolean())
            })
            .collect();
    }
    Value::NodeSet(current)
}

fn qname_matches(
    ns: Option<&str>,
    local: &str,
    test_name: &str,
    namespaces: &HashMap<String, String>,
) -> bool {
    match test_name.split_once(':') {
        Some((prefix, name)) => match namespaces.get(prefix) {
            Some(uri) => ns == Some(uri.as_str()) && local == name,
            None => false,
        },
        None => ns.is_none() && local == test_name,
    }
}

fn name_test_matches_elem(n: Node, test: &NameTest, namespaces: &HashMap<String, String>) -> bool {
    if !n.is_element() {
        return false;
    }
    match test {
        NameTest::Any => true,
        NameTest::Name(qn) => qname_matches(
            n.tag_name().namespace(),
            n.tag_name().name(),
            qn,
            namespaces,
        ),
    }
}

fn axis_step<'a, 'input>(
    node: &NodeRef<'a, 'input>,
    axis: Axis,
    test: &NameTest,
    namespaces: &HashMap<String, String>,
) -> Vec<NodeRef<'a, 'input>> {
    let elem = match node {
        NodeRef::Elem(n) => *n,
        // Attribute nodes have no further navigable structure in our
        // rule set (they're always used for their value); only the
        // self/parent axes make sense and aren't needed here.
        NodeRef::Attr { .. } => return Vec::new(),
    };
    match axis {
        Axis::Child => elem
            .children()
            .filter(|c| name_test_matches_elem(*c, test, namespaces))
            .map(NodeRef::Elem)
            .collect(),
        Axis::Attribute => elem
            .attributes()
            .filter(|a| match test {
                NameTest::Any => true,
                NameTest::Name(qn) => qname_matches(a.namespace(), a.name(), qn, namespaces),
            })
            .map(|a| NodeRef::Attr {
                owner: elem,
                ns: a.namespace().map(str::to_string),
                local: a.name().to_string(),
                value: a.value().to_string(),
            })
            .collect(),
        Axis::DescendantOrSelf => {
            let mut out = Vec::new();
            if name_test_matches_elem(elem, test, namespaces) || matches!(test, NameTest::Any) {
                // descendant-or-self::node() always includes self, regardless
                // of name test (it's the desugaring of `//`); a *named*
                // descendant-or-self test additionally requires a name match.
                if matches!(test, NameTest::Any) || name_test_matches_elem(elem, test, namespaces) {
                    out.push(NodeRef::Elem(elem));
                }
            }
            for d in elem.descendants().skip(1) {
                if name_test_matches_elem(d, test, namespaces) || matches!(test, NameTest::Any) {
                    out.push(NodeRef::Elem(d));
                }
            }
            out
        }
        Axis::Descendant => elem
            .descendants()
            .skip(1)
            .filter(|d| name_test_matches_elem(*d, test, namespaces))
            .map(NodeRef::Elem)
            .collect(),
        Axis::Parent => elem
            .parent()
            .filter(|p| name_test_matches_elem(*p, test, namespaces))
            .map(NodeRef::Elem)
            .into_iter()
            .collect(),
        Axis::Ancestor => elem
            .ancestors()
            .skip(1)
            .filter(|a| name_test_matches_elem(*a, test, namespaces))
            .map(NodeRef::Elem)
            .collect(),
        Axis::AncestorOrSelf => elem
            .ancestors()
            .filter(|a| name_test_matches_elem(*a, test, namespaces))
            .map(NodeRef::Elem)
            .collect(),
        Axis::SelfAxis => {
            if matches!(test, NameTest::Any) || name_test_matches_elem(elem, test, namespaces) {
                vec![NodeRef::Elem(elem)]
            } else {
                Vec::new()
            }
        }
    }
}

fn nodeset_matches_scalar(ns: &[NodeRef], other: &Value) -> bool {
    match other {
        Value::Number(n) => ns
            .iter()
            .any(|node| node.string_value().trim().parse::<f64>().ok() == Some(*n)),
        Value::Boolean(b) => (!ns.is_empty()) == *b,
        _ => {
            let s = other.to_string_value();
            ns.iter().any(|node| node.string_value() == s)
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::NodeSet(x), Value::NodeSet(y)) => x
            .iter()
            .any(|xn| y.iter().any(|yn| xn.string_value() == yn.string_value())),
        (Value::NodeSet(ns), other) | (other, Value::NodeSet(ns)) => {
            nodeset_matches_scalar(ns, other)
        }
        (Value::Boolean(_), _) | (_, Value::Boolean(_)) => a.to_boolean() == b.to_boolean(),
        (Value::Number(_), _) | (_, Value::Number(_)) => a.to_number() == b.to_number(),
        _ => a.to_string_value() == b.to_string_value(),
    }
}

fn compare_relational(a: &Value, b: &Value, op: BinOp) -> bool {
    fn cmp(x: f64, y: f64, op: BinOp) -> bool {
        match op {
            BinOp::Lt => x < y,
            BinOp::Le => x <= y,
            BinOp::Gt => x > y,
            BinOp::Ge => x >= y,
            _ => false,
        }
    }
    match (a, b) {
        (Value::NodeSet(ns), _) => ns.iter().any(|n| {
            cmp(
                n.string_value().trim().parse().unwrap_or(f64::NAN),
                b.to_number(),
                op,
            )
        }),
        (_, Value::NodeSet(ns)) => ns.iter().any(|n| {
            cmp(
                a.to_number(),
                n.string_value().trim().parse().unwrap_or(f64::NAN),
                op,
            )
        }),
        _ => cmp(a.to_number(), b.to_number(), op),
    }
}

fn eval_binop<'a, 'input>(
    a: &Expr,
    op: BinOp,
    b: &Expr,
    env: &Env<'a, 'input, '_>,
    nodes: &[NodeRef<'a, 'input>],
) -> Value<'a, 'input> {
    match op {
        BinOp::And => {
            Value::Boolean(eval(a, env, nodes).to_boolean() && eval(b, env, nodes).to_boolean())
        }
        BinOp::Or => {
            Value::Boolean(eval(a, env, nodes).to_boolean() || eval(b, env, nodes).to_boolean())
        }
        BinOp::Eq => Value::Boolean(values_equal(&eval(a, env, nodes), &eval(b, env, nodes))),
        BinOp::Ne => Value::Boolean(!values_equal(&eval(a, env, nodes), &eval(b, env, nodes))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Value::Boolean(compare_relational(
            &eval(a, env, nodes),
            &eval(b, env, nodes),
            op,
        )),
        BinOp::Add => {
            Value::Number(eval(a, env, nodes).to_number() + eval(b, env, nodes).to_number())
        }
        BinOp::Sub => {
            Value::Number(eval(a, env, nodes).to_number() - eval(b, env, nodes).to_number())
        }
        BinOp::Mul => {
            Value::Number(eval(a, env, nodes).to_number() * eval(b, env, nodes).to_number())
        }
        BinOp::Div => {
            Value::Number(eval(a, env, nodes).to_number() / eval(b, env, nodes).to_number())
        }
        BinOp::Mod => {
            Value::Number(eval(a, env, nodes).to_number() % eval(b, env, nodes).to_number())
        }
    }
}

fn eval_call<'a, 'input>(
    name: &str,
    args: &[Expr],
    env: &Env<'a, 'input, '_>,
    nodes: &[NodeRef<'a, 'input>],
) -> Value<'a, 'input> {
    let arg = |i: usize| args.get(i).map(|e| eval(e, env, nodes));
    match name {
        "current" => Value::NodeSet(vec![env.current.clone()]),
        "count" => Value::Number(arg(0).map(|v| v.into_nodeset().len()).unwrap_or(0) as f64),
        "not" => Value::Boolean(!arg(0).map(|v| v.to_boolean()).unwrap_or(false)),
        "boolean" => Value::Boolean(arg(0).map(|v| v.to_boolean()).unwrap_or(false)),
        "true" => Value::Boolean(true),
        "false" => Value::Boolean(false),
        "exists" => Value::Boolean(arg(0).map(|v| v.to_boolean()).unwrap_or(false)),
        "empty" => Value::Boolean(!arg(0).map(|v| v.to_boolean()).unwrap_or(false)),
        "string" => Value::String(arg(0).map(|v| v.to_string_value()).unwrap_or_default()),
        "string-length" => {
            let s = arg(0)
                .map(|v| v.to_string_value())
                .unwrap_or_else(|| current_string_value(env));
            Value::Number(s.chars().count() as f64)
        }
        "normalize-space" => {
            let s = arg(0)
                .map(|v| v.to_string_value())
                .unwrap_or_else(|| current_string_value(env));
            Value::String(s.split_whitespace().collect::<Vec<_>>().join(" "))
        }
        "lower-case" => Value::String(
            arg(0)
                .map(|v| v.to_string_value())
                .unwrap_or_default()
                .to_lowercase(),
        ),
        "upper-case" => Value::String(
            arg(0)
                .map(|v| v.to_string_value())
                .unwrap_or_default()
                .to_uppercase(),
        ),
        "concat" => Value::String(
            args.iter()
                .map(|e| eval(e, env, nodes).to_string_value())
                .collect::<Vec<_>>()
                .concat(),
        ),
        "contains" => {
            let (h, n) = (
                arg(0).unwrap_or(Value::String(String::new())),
                arg(1).unwrap_or(Value::String(String::new())),
            );
            Value::Boolean(h.to_string_value().contains(&n.to_string_value()))
        }
        "starts-with" => {
            let (h, n) = (
                arg(0).unwrap_or(Value::String(String::new())),
                arg(1).unwrap_or(Value::String(String::new())),
            );
            Value::Boolean(h.to_string_value().starts_with(&n.to_string_value()))
        }
        "substring-before" => {
            let (h, n) = (
                arg(0).map(|v| v.to_string_value()).unwrap_or_default(),
                arg(1).map(|v| v.to_string_value()).unwrap_or_default(),
            );
            Value::String(h.find(&n).map(|i| h[..i].to_string()).unwrap_or_default())
        }
        "substring-after" => {
            let (h, n) = (
                arg(0).map(|v| v.to_string_value()).unwrap_or_default(),
                arg(1).map(|v| v.to_string_value()).unwrap_or_default(),
            );
            Value::String(
                h.find(&n)
                    .map(|i| h[i + n.len()..].to_string())
                    .unwrap_or_default(),
            )
        }
        "substring" => {
            let s = arg(0).map(|v| v.to_string_value()).unwrap_or_default();
            let chars: Vec<char> = s.chars().collect();
            // XPath substring is 1-based with round-to-nearest length semantics;
            // we implement the common case (positive integer start/length).
            let start = arg(1).map(|v| v.to_number()).unwrap_or(1.0).round() as i64;
            let take = args
                .get(2)
                .map(|e| eval(e, env, nodes).to_number().round() as i64);
            let start0 = (start - 1).max(0) as usize;
            let end0 = match take {
                Some(len) => ((start - 1) + len).max(0) as usize,
                None => chars.len(),
            };
            let end0 = end0.min(chars.len());
            if start0 >= chars.len() || start0 >= end0 {
                Value::String(String::new())
            } else {
                Value::String(chars[start0..end0].iter().collect())
            }
        }
        _ => Value::NodeSet(Vec::new()), // unknown function: never-matching, non-fatal
    }
}

fn current_string_value(env: &Env) -> String {
    env.current.string_value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::parser::parse;

    fn eval_bool_on(xml: &str, xpath: &str) -> bool {
        let doc = roxmltree::Document::parse(xml).unwrap();
        let expr = parse(xpath).unwrap();
        let vars = HashMap::new();
        let mut namespaces = HashMap::new();
        namespaces.insert("opf".to_string(), "urn:test:opf".to_string());
        let env = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(doc.root_element()),
            vars: &vars,
            namespaces: &namespaces,
        };
        eval_boolean(&expr, &env, doc.root_element())
    }

    #[test]
    fn attribute_existence_and_count() {
        let xml = r#"<opf:package xmlns:opf="urn:test:opf"><opf:metadata id="a"/><opf:metadata id="b"/></opf:package>"#;
        assert!(eval_bool_on(xml, "@xmlns:opf") == false); // xmlns declarations aren't real attributes in roxmltree
        assert!(eval_bool_on(xml, "count(opf:metadata) = 2"));
        assert!(eval_bool_on(xml, "count(opf:metadata) = 3") == false);
    }

    #[test]
    fn id_uniqueness_shape() {
        // exactly the $id-set[@id = current()/@id] pattern id-uniqueness needs
        let xml = r#"<opf:package xmlns:opf="urn:test:opf"><a id="x"/><b id="x"/><c id="y"/></opf:package>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut vars = HashMap::new();
        let id_set_expr = parse("//*[@id]").unwrap();
        let root_env_namespaces: HashMap<String, String> = HashMap::new();
        let root_env = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(doc.root_element()),
            vars: &vars,
            namespaces: &root_env_namespaces,
        };
        let id_set_value = eval(
            &id_set_expr,
            &root_env,
            &[NodeRef::Elem(doc.root_element())],
        );
        vars.insert("id-set".to_string(), id_set_value);

        let test_expr = parse("count($id-set[@id = current()/@id]) = 1").unwrap();
        let a_node = doc
            .root_element()
            .children()
            .find(|n| n.tag_name().name() == "a")
            .unwrap();
        let b_node = doc
            .root_element()
            .children()
            .find(|n| n.tag_name().name() == "b")
            .unwrap();
        let c_node = doc
            .root_element()
            .children()
            .find(|n| n.tag_name().name() == "c")
            .unwrap();

        let env_a = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(a_node),
            vars: &vars,
            namespaces: &root_env_namespaces,
        };
        assert!(
            !eval_boolean(&test_expr, &env_a, a_node),
            "duplicate id should fail uniqueness"
        );
        let env_b = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(b_node),
            vars: &vars,
            namespaces: &root_env_namespaces,
        };
        assert!(
            !eval_boolean(&test_expr, &env_b, b_node),
            "duplicate id should fail uniqueness"
        );
        let env_c = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(c_node),
            vars: &vars,
            namespaces: &root_env_namespaces,
        };
        assert!(
            eval_boolean(&test_expr, &env_c, c_node),
            "unique id should pass"
        );
    }

    #[test]
    fn ancestor_axis_eval() {
        let xml = r##"<opf:package xmlns:opf="urn:test:opf"><opf:collection><opf:meta refines="#x"/></opf:collection><opf:meta id="top"/></opf:package>"##;
        assert!(eval_bool_on(
            xml,
            "count(//opf:meta[ancestor::opf:collection]) = 1"
        ));
    }

    #[test]
    fn string_functions() {
        let xml = "<root xmlns:opf='urn:test:opf'><a>  hello   world  </a></root>";
        assert!(eval_bool_on(xml, "normalize-space(//a) = 'hello world'"));
        assert!(eval_bool_on(xml, "contains(//a, 'wor')"));
        assert!(eval_bool_on(
            xml,
            "starts-with(normalize-space(//a), 'hello')"
        ));
    }

    #[test]
    fn unprefixed_name_means_null_namespace() {
        // The document's elements are in the default (non-empty) namespace,
        // but an unprefixed test must NOT match them (a real XPath 1.0 gotcha).
        let xml = r#"<root xmlns="urn:default"><child/></root>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let expr = parse("child").unwrap();
        let vars = HashMap::new();
        let namespaces = HashMap::new();
        let env = Env {
            root: doc.root_element(),
            current: NodeRef::Elem(doc.root_element()),
            vars: &vars,
            namespaces: &namespaces,
        };
        let v = eval(&expr, &env, &[NodeRef::Elem(doc.root_element())]);
        assert_eq!(v.into_nodeset().len(), 0);
    }
}
