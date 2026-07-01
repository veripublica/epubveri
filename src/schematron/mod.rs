//! Schematron: loading `<schema>`/`<pattern>`/`<rule>`/`<let>`/`<assert>`/
//! `<report>` documents and running them against a `roxmltree` document,
//! using the `crate::xpath` engine (an XPath 1.0 core subset — see that
//! module's docs for exactly what's in/out of scope) for `context`/`test`/
//! `value`/`select` expressions.
//!
//! **Context selection semantics:** a real Schematron `context` is a
//! "match pattern" (XSLT-style), not an ordinary XPath expression — e.g.
//! `context="opf:package[@unique-identifier]"` can and does match the
//! *document's own root element*, which a literal `//opf:package` (real
//! XPath) never would (`//foo` only ever finds `foo` as somebody's child).
//! So context selection walks every node and checks the step sequence
//! *backwards* through each candidate's ancestor chain (`select_context_nodes`/
//! `matches_context_pattern`), rather than reusing the forward path-navigation
//! `crate::xpath::eval` uses for `test`/`value` expressions.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::xpath::ast::{Axis, Expr, NameTest, Path, PathStart, Step};
use crate::xpath::{eval, Env, NodeRef, Value};

const SCH_NS: &str = "http://purl.oclc.org/dsdl/schematron";

#[derive(Debug, Clone)]
pub struct Let {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    Assert,
    Report,
}

#[derive(Debug, Clone)]
pub enum MessagePart {
    Text(String),
    ValueOf(Expr),
}

#[derive(Debug, Clone)]
pub struct Check {
    pub kind: CheckKind,
    pub test: Expr,
    pub message: Vec<MessagePart>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub context: Path,
    pub lets: Vec<Let>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Default)]
pub struct Pattern {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub namespaces: HashMap<String, String>,
    pub lets: Vec<Let>,
    pub patterns: Vec<Pattern>,
}

// --- loader ---

fn is_sch(n: Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(SCH_NS)
}
fn lname<'a>(n: Node<'a, 'a>) -> &'a str {
    n.tag_name().name()
}
fn sch_children<'a>(n: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    n.children().filter(|c| is_sch(*c))
}

/// Our own EPUB package-document Schematron rules, embedded at build time
/// (committed under the project license; authored from scratch — not
/// derived from epubcheck's real `.sch` files). See `schemas/package.sch`
/// for the scope/design notes.
pub const PACKAGE_SCH: &str = include_str!("../../schemas/package.sch");

/// Load the built-in package-document Schematron schema.
pub fn package_schema() -> Schema {
    load(PACKAGE_SCH).expect("built-in package.sch must parse")
}

pub fn load(xml: &str) -> Result<Schema, String> {
    let doc = Document::parse(xml).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if !is_sch(root) || lname(root) != "schema" {
        return Err("expected a Schematron <schema> root element".to_string());
    }
    let mut namespaces = HashMap::new();
    for ns in sch_children(root).filter(|c| lname(*c) == "ns") {
        let prefix = ns
            .attribute("prefix")
            .ok_or("<ns> is missing a 'prefix' attribute")?;
        let uri = ns
            .attribute("uri")
            .ok_or("<ns> is missing a 'uri' attribute")?;
        namespaces.insert(prefix.to_string(), uri.to_string());
    }
    let mut lets = Vec::new();
    for l in sch_children(root).filter(|c| lname(*c) == "let") {
        lets.push(parse_let(l)?);
    }
    let mut patterns = Vec::new();
    for p in sch_children(root).filter(|c| lname(*c) == "pattern") {
        patterns.push(parse_pattern(p)?);
    }
    Ok(Schema {
        namespaces,
        lets,
        patterns,
    })
}

fn parse_let(n: Node) -> Result<Let, String> {
    let name = n
        .attribute("name")
        .ok_or("<let> is missing a 'name' attribute")?
        .to_string();
    let value_str = n
        .attribute("value")
        .ok_or("<let> is missing a 'value' attribute")?;
    let value = crate::xpath::parse(value_str)
        .map_err(|e| format!("bad <let> value expression '{value_str}': {e}"))?;
    Ok(Let { name, value })
}

fn parse_pattern(n: Node) -> Result<Pattern, String> {
    let mut rules = Vec::new();
    for r in sch_children(n).filter(|c| lname(*c) == "rule") {
        rules.push(parse_rule(r)?);
    }
    Ok(Pattern { rules })
}

fn parse_rule(n: Node) -> Result<Rule, String> {
    let context_str = n
        .attribute("context")
        .ok_or("<rule> is missing a 'context' attribute")?;
    let context_expr = crate::xpath::parse(context_str)
        .map_err(|e| format!("bad rule context '{context_str}': {e}"))?;
    let context = match context_expr {
        Expr::Path(p) => p,
        other => Path {
            start: PathStart::Expr(Box::new(other)),
            steps: Vec::new(),
        },
    };
    let mut lets = Vec::new();
    let mut checks = Vec::new();
    for c in sch_children(n) {
        match lname(c) {
            "let" => lets.push(parse_let(c)?),
            "assert" => checks.push(parse_check(c, CheckKind::Assert)?),
            "report" => checks.push(parse_check(c, CheckKind::Report)?),
            _ => {}
        }
    }
    Ok(Rule {
        context,
        lets,
        checks,
    })
}

fn parse_check(n: Node, kind: CheckKind) -> Result<Check, String> {
    let test_str = n
        .attribute("test")
        .ok_or("<assert>/<report> is missing a 'test' attribute")?;
    let test = crate::xpath::parse(test_str)
        .map_err(|e| format!("bad test expression '{test_str}': {e}"))?;
    let mut message = Vec::new();
    for child in n.children() {
        if child.is_text() {
            if let Some(t) = child.text() {
                message.push(MessagePart::Text(t.to_string()));
            }
        } else if is_sch(child) && lname(child) == "value-of" {
            let select_str = child
                .attribute("select")
                .ok_or("<value-of> is missing a 'select' attribute")?;
            let select = crate::xpath::parse(select_str)
                .map_err(|e| format!("bad value-of select '{select_str}': {e}"))?;
            message.push(MessagePart::ValueOf(select));
        }
    }
    Ok(Check {
        kind,
        test,
        message,
    })
}

// --- executor ---

/// Run a loaded schema against a document, returning one message per fired
/// check (a failing `assert` or a true `report`), in document/pattern/rule
/// order. The caller (e.g. `opf::check`) decides how to report these (this
/// project reports Schematron findings as `RSC-005`, matching how epubcheck
/// itself surfaces nearly all of them under that one catch-all code).
pub fn run<'input>(schema: &Schema, doc: &Document<'input>) -> Vec<String> {
    let root = doc.root_element();
    let namespaces = &schema.namespaces;

    let mut schema_vars: HashMap<String, Value> = HashMap::new();
    for l in &schema.lets {
        let v = {
            let env = Env {
                root,
                current: NodeRef::Elem(root),
                vars: &schema_vars,
                namespaces,
            };
            eval(&l.value, &env, &[NodeRef::Elem(root)])
        };
        schema_vars.insert(l.name.clone(), v);
    }

    let mut messages = Vec::new();
    for pattern in &schema.patterns {
        for rule in &pattern.rules {
            for node in select_context_nodes(&rule.context, root, namespaces) {
                let mut rule_vars = schema_vars.clone();
                for l in &rule.lets {
                    let v = {
                        let env = Env {
                            root,
                            current: NodeRef::Elem(node),
                            vars: &rule_vars,
                            namespaces,
                        };
                        eval(&l.value, &env, &[NodeRef::Elem(node)])
                    };
                    rule_vars.insert(l.name.clone(), v);
                }
                let env = Env {
                    root,
                    current: NodeRef::Elem(node),
                    vars: &rule_vars,
                    namespaces,
                };
                for check in &rule.checks {
                    let test_true = crate::xpath::eval_boolean(&check.test, &env, node);
                    let fires = match check.kind {
                        CheckKind::Assert => !test_true,
                        CheckKind::Report => test_true,
                    };
                    if fires {
                        messages.push(render_message(&check.message, &env, node));
                    }
                }
            }
        }
    }
    messages
}

/// Select every node in the document matching a Schematron `context`
/// pattern. Unlike a real XPath `//pattern` expression (which can never
/// match the document's own root element — `//foo` only ever finds `foo`
/// as *someone's child*), a Schematron match-pattern context legitimately
/// can and does match the root itself (e.g. real epubcheck's own
/// `context="opf:package[@unique-identifier]"` targets the root
/// `<package>` element). So this walks every node and, per candidate,
/// checks the step sequence *backwards* through its ancestor chain, rather
/// than reusing the forward path-navigation machinery `eval` uses for
/// ordinary path expressions.
fn select_context_nodes<'a, 'input>(
    context: &Path,
    root: Node<'a, 'input>,
    namespaces: &HashMap<String, String>,
) -> Vec<Node<'a, 'input>> {
    if let PathStart::Expr(_) = &context.start {
        // Not a location path at all (e.g. a bare variable) — no sensible
        // "everywhere in the document" search space; fall back to empty.
        return Vec::new();
    }
    root.descendants()
        .filter(|n| n.is_element() && matches_context_pattern(*n, &context.steps, root, namespaces))
        .collect()
}

fn matches_context_pattern(
    node: Node,
    steps: &[Step],
    root: Node,
    namespaces: &HashMap<String, String>,
) -> bool {
    let Some(last) = steps.last() else {
        return true;
    };
    if last.axis != Axis::Child && last.axis != Axis::DescendantOrSelf {
        // Context patterns are always element-shaped in the schemas we
        // author; other axes (e.g. `@attr` as a whole context) aren't a
        // shape epubcheck's real rules use either.
        return false;
    }
    if !name_test_matches_for_context(node, &last.test, namespaces) {
        return false;
    }
    let empty_vars = HashMap::new();
    for pred in &last.predicates {
        let env = Env {
            root,
            current: NodeRef::Elem(node),
            vars: &empty_vars,
            namespaces,
        };
        if !crate::xpath::eval_boolean(pred, &env, node) {
            return false;
        }
    }
    if steps.len() == 1 {
        true
    } else {
        match node.parent() {
            Some(parent) => {
                matches_context_pattern(parent, &steps[..steps.len() - 1], root, namespaces)
            }
            None => false,
        }
    }
}

fn name_test_matches_for_context(
    node: Node,
    test: &NameTest,
    namespaces: &HashMap<String, String>,
) -> bool {
    match test {
        NameTest::Any => true,
        NameTest::Name(qn) => match qn.split_once(':') {
            Some((prefix, name)) => match namespaces.get(prefix) {
                Some(uri) => {
                    node.tag_name().namespace() == Some(uri.as_str())
                        && node.tag_name().name() == name
                }
                None => false,
            },
            None => node.tag_name().namespace().is_none() && node.tag_name().name() == qn,
        },
    }
}

fn render_message(parts: &[MessagePart], env: &Env, node: Node) -> String {
    let mut s = String::new();
    for p in parts {
        match p {
            MessagePart::Text(t) => s.push_str(t),
            MessagePart::ValueOf(e) => {
                s.push_str(&eval(e, env, &[NodeRef::Elem(node)]).to_string_value())
            }
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_id_detected() {
        let sch = r#"
            <schema xmlns="http://purl.oclc.org/dsdl/schematron">
              <let name="id-set" value="//*[@id]"/>
              <pattern>
                <rule context="*[@id]">
                  <assert test="count($id-set[@id = current()/@id]) = 1">Duplicate id</assert>
                </rule>
              </pattern>
            </schema>
        "#;
        let schema = load(sch).unwrap();
        let doc = Document::parse(r#"<root><a id="x"/><b id="x"/><c id="y"/></root>"#).unwrap();
        let messages = run(&schema, &doc);
        assert_eq!(
            messages,
            vec!["Duplicate id".to_string(), "Duplicate id".to_string()]
        );
    }

    #[test]
    fn clean_document_produces_no_messages() {
        let sch = r#"
            <schema xmlns="http://purl.oclc.org/dsdl/schematron">
              <let name="id-set" value="//*[@id]"/>
              <pattern>
                <rule context="*[@id]">
                  <assert test="count($id-set[@id = current()/@id]) = 1">Duplicate id</assert>
                </rule>
              </pattern>
            </schema>
        "#;
        let schema = load(sch).unwrap();
        let doc = Document::parse(r#"<root><a id="x"/><b id="y"/></root>"#).unwrap();
        assert!(run(&schema, &doc).is_empty());
    }

    #[test]
    fn report_fires_when_test_is_true() {
        let sch = r#"
            <schema xmlns="http://purl.oclc.org/dsdl/schematron">
              <ns uri="urn:test:epub" prefix="epub"/>
              <pattern>
                <rule context="epub:switch">
                  <report test="true()">WARNING: epub:switch is deprecated</report>
                </rule>
              </pattern>
            </schema>
        "#;
        let schema = load(sch).unwrap();
        let doc =
            Document::parse(r#"<root xmlns:epub="urn:test:epub"><epub:switch/></root>"#).unwrap();
        assert_eq!(
            run(&schema, &doc),
            vec!["WARNING: epub:switch is deprecated".to_string()]
        );
    }

    #[test]
    fn value_of_in_message() {
        let sch = r#"
            <schema xmlns="http://purl.oclc.org/dsdl/schematron">
              <pattern>
                <rule context="a">
                  <report test="true()">bad id "<value-of select="@id"/>"</report>
                </rule>
              </pattern>
            </schema>
        "#;
        let schema = load(sch).unwrap();
        let doc = Document::parse(r#"<root><a id="x"/></root>"#).unwrap();
        assert_eq!(run(&schema, &doc), vec!["bad id \"x\"".to_string()]);
    }

    #[test]
    fn namespaced_context_uses_schema_ns_declarations_not_document_defaults() {
        let sch = r#"
            <schema xmlns="http://purl.oclc.org/dsdl/schematron">
              <ns uri="urn:test:opf" prefix="opf"/>
              <pattern>
                <rule context="opf:package[@unique-identifier]">
                  <assert test="/opf:package/opf:metadata/opf:identifier[@id = current()/@unique-identifier]"
                    >unique-identifier does not resolve</assert>
                </rule>
              </pattern>
            </schema>
        "#;
        let schema = load(sch).unwrap();
        let ok_doc = Document::parse(
            r#"<opf:package xmlns:opf="urn:test:opf" unique-identifier="id"><opf:metadata><opf:identifier id="id"/></opf:metadata></opf:package>"#,
        )
        .unwrap();
        assert!(run(&schema, &ok_doc).is_empty());

        let bad_doc = Document::parse(
            r#"<opf:package xmlns:opf="urn:test:opf" unique-identifier="missing"><opf:metadata><opf:identifier id="id"/></opf:metadata></opf:package>"#,
        )
        .unwrap();
        assert_eq!(
            run(&schema, &bad_doc),
            vec!["unique-identifier does not resolve".to_string()]
        );
    }
}
