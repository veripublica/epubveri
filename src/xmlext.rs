//! A tiny extension trait pinning down *namespace-less* attribute access.
//!
//! Why this exists: `roxmltree` 0.21 changed `Node::attribute(&str)` /
//! `has_attribute(&str)` to match attributes by **local name**, ignoring the
//! namespace (aligning them with `has_tag_name`'s `&str` form). Under that
//! semantic, `attribute("lang")` also matches `xml:lang`, `attribute("href")`
//! also matches `xlink:href`, and so on — which silently broke the checks that
//! distinguish a plain attribute from its namespaced sibling (found via the
//! corpus: the `lang`/`xml:lang` mismatch check went silent and two valid SMIL
//! fixtures drew spurious RSC-005s).
//!
//! Every call site in this crate that means "the attribute named X **with no
//! namespace**" uses these methods instead of the bare-`&str` forms, so the
//! intent is explicit and the semantics are identical on every `roxmltree`
//! version — the iterator filter below doesn't depend on the lookup methods'
//! matching rules at all. Namespaced access stays on the stock tuple form
//! (`attribute((NS, "name"))`), whose exact-namespace matching is unchanged.

use std::collections::BTreeMap;

/// Namespace-explicit attribute accessors for [`roxmltree::Node`].
pub trait NodeExt<'a> {
    /// The value of the attribute with local name `name` and **no namespace**,
    /// like `attribute(&str)` behaved up to roxmltree 0.20.
    fn attr_no_ns(&self, name: &str) -> Option<&'a str>;
    /// Whether an attribute with local name `name` and **no namespace** exists.
    fn has_attr_no_ns(&self, name: &str) -> bool {
        self.attr_no_ns(name).is_some()
    }
}

impl<'a, 'input: 'a> NodeExt<'a> for roxmltree::Node<'a, 'input> {
    fn attr_no_ns(&self, name: &str) -> Option<&'a str> {
        self.attributes()
            .find(|a| a.namespace().is_none() && a.name() == name)
            .map(|a| a.value())
    }
}

/// A machine-resolvable, XPath/lxml-ElementPath-style location for a
/// node-anchored finding (issue #18), plus the namespace bindings needed to
/// resolve it against a parsed tree.
///
/// The path is rooted with 1-based sibling indices, e.g.
/// `/package/metadata/dc:contributor[1]`, optionally ending in an `/@prefix:name`
/// attribute or `/text()` step. Names carry the **source prefix** exactly as the
/// document authored them (`dc:contributor`, `opf:role`); a default-namespace
/// element stays bare (`package`, `html`). Because EPUB documents are always
/// namespaced (XHTML and OPF both use a *default* namespace) and XPath 1.0 has
/// no default-namespace concept, a bare path is not resolvable on its own — so
/// [`namespaces`](Self::namespaces) travels alongside, mapping each prefix to
/// its URI (the default namespace under the empty-string key), letting a strict
/// engine register the namespace context instead of guessing.
#[derive(Debug, Clone)]
pub struct NodePath {
    pub path: String,
    /// Prefix -> namespace-URI bindings used by `path`. Default namespace under
    /// the `""` key. Empty when the path touches no namespaced name.
    pub namespaces: BTreeMap<String, String>,
}

/// Build the element path to `node` (issue #18). `node` is the element the
/// finding is anchored at; the resulting path targets that element.
pub(crate) fn node_path(node: roxmltree::Node) -> NodePath {
    let mut namespaces = BTreeMap::new();
    // `ancestors()` yields self first, then parents; the document root is not an
    // element, so filtering to elements drops it (no bogus leading segment).
    let mut segments: Vec<String> = node
        .ancestors()
        .filter(roxmltree::Node::is_element)
        .map(|el| element_segment(el, &mut namespaces))
        .collect();
    segments.reverse();
    NodePath {
        path: format!("/{}", segments.join("/")),
        namespaces,
    }
}

/// Like [`node_path`], but the finding is about a specific attribute of `node`;
/// the path ends in an `/@prefix:name` (or bare `/@name`) step. Pass the
/// [`roxmltree::Attribute`] in hand so its namespace is resolved exactly, not
/// guessed from a local name.
pub(crate) fn node_path_attr(node: roxmltree::Node, attr: roxmltree::Attribute) -> NodePath {
    let mut np = node_path(node);
    // An unprefixed attribute is never in a namespace (the default namespace
    // never applies to attributes), so it's a bare `@name`. A namespaced
    // attribute (`opf:role`) was necessarily authored with a prefix, so a
    // prefixed binding is in scope — find one (never the same-URI default, which
    // would wrongly collapse `@opf:role` to `@role`).
    let step = match attr.namespace() {
        Some(uri) => match prefixed_binding(node, uri) {
            Some(prefix) => {
                np.namespaces.insert(prefix.to_string(), uri.to_string());
                format!("@{prefix}:{}", attr.name())
            }
            None => format!("@{}", attr.name()),
        },
        None => format!("@{}", attr.name()),
    };
    np.path.push('/');
    np.path.push_str(&step);
    np
}

/// One `name[index]` path segment for an element, recording any namespace
/// binding the name relies on.
fn element_segment(el: roxmltree::Node, namespaces: &mut BTreeMap<String, String>) -> String {
    let name = el.tag_name();
    let local = name.name();
    let qname = match name.namespace() {
        None => local.to_string(),
        // Prefer the bare form when the name resolves through the in-scope
        // default namespace — that's how default-namespaced content (XHTML, OPF)
        // is authored, even when the same URI *also* has an explicit prefix
        // bound. The path stays resolvable either way; this keeps it faithful.
        Some(uri) if el.default_namespace() == Some(uri) => {
            namespaces.insert(String::new(), uri.to_string());
            local.to_string()
        }
        // A genuinely prefixed name (`dc:contributor`): record prefix -> URI.
        Some(uri) => match prefixed_binding(el, uri) {
            Some(prefix) => {
                namespaces.insert(prefix.to_string(), uri.to_string());
                format!("{prefix}:{local}")
            }
            // Namespaced but no in-scope binding maps to it (not reachable for a
            // parsed tree); stay resolvable by recording it as the default.
            None => {
                namespaces.insert(String::new(), uri.to_string());
                local.to_string()
            }
        },
    };
    format!("{qname}[{}]", element_index(el))
}

/// A prefix (never the default/`xmlns`) bound to `uri` in scope at `node`. Used
/// where a real prefix is required — namespaced attributes, and prefixed element
/// names — so a same-URI default binding never collapses `dc:x`/`@opf:y`.
fn prefixed_binding<'a, 'input: 'a>(
    node: roxmltree::Node<'a, 'input>,
    uri: &str,
) -> Option<&'input str> {
    node.namespaces()
        .find_map(|ns| ns.name().filter(|_| ns.uri() == uri))
}

/// 1-based position of `el` among its same-expanded-name element siblings, as
/// XPath's `p[3]` predicate counts (namespace + local name must both match).
fn element_index(el: roxmltree::Node) -> usize {
    let name = el.tag_name();
    match el.parent() {
        Some(parent) => {
            parent
                .children()
                .filter(|c| c.is_element() && c.tag_name() == name)
                .take_while(|c| *c != el)
                .count()
                + 1
        }
        None => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeExt, node_path, node_path_attr};

    fn find<'a>(doc: &'a roxmltree::Document, local: &str) -> roxmltree::Node<'a, 'a> {
        doc.descendants()
            .find(|n| n.is_element() && n.tag_name().name() == local)
            .unwrap()
    }

    #[test]
    fn default_namespace_element_path_stays_bare_but_records_the_uri() {
        // XHTML: html/body/p all live in the default namespace. Names are bare,
        // but the URI is still needed to resolve them (XPath 1.0 has no default
        // namespace), so it's recorded under the "" key.
        let doc = roxmltree::Document::parse(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p/><p/></body></html>"#,
        )
        .unwrap();
        let second_p = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "p")
            .nth(1)
            .unwrap();
        let np = node_path(second_p);
        assert_eq!(np.path, "/html[1]/body[1]/p[2]");
        assert_eq!(
            np.namespaces.get(""),
            Some(&"http://www.w3.org/1999/xhtml".to_string())
        );
    }

    #[test]
    fn prefixed_element_path_carries_the_source_prefix_and_binding() {
        let doc = roxmltree::Document::parse(
            r#"<package xmlns="http://www.idpf.org/2007/opf">
                 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                   <dc:title>a</dc:title>
                   <dc:contributor>b</dc:contributor>
                 </metadata>
               </package>"#,
        )
        .unwrap();
        let np = node_path(find(&doc, "contributor"));
        assert_eq!(np.path, "/package[1]/metadata[1]/dc:contributor[1]");
        assert_eq!(
            np.namespaces.get("dc"),
            Some(&"http://purl.org/dc/elements/1.1/".to_string())
        );
        // The default (opf) namespace the bare `package`/`metadata` rely on is
        // recorded too.
        assert_eq!(
            np.namespaces.get(""),
            Some(&"http://www.idpf.org/2007/opf".to_string())
        );
    }

    #[test]
    fn attribute_target_pins_the_offending_namespaced_attribute() {
        // jenstroeger's #18 case: `attribute "opf:role" not allowed here`.
        let doc = roxmltree::Document::parse(
            r#"<package xmlns="http://www.idpf.org/2007/opf">
                 <metadata xmlns:opf="http://www.idpf.org/2007/opf"
                           xmlns:dc="http://purl.org/dc/elements/1.1/">
                   <dc:contributor opf:role="bkp">x</dc:contributor>
                 </metadata>
               </package>"#,
        )
        .unwrap();
        let contributor = find(&doc, "contributor");
        let role = contributor
            .attributes()
            .find(|a| a.name() == "role")
            .unwrap();
        let np = node_path_attr(contributor, role);
        assert_eq!(
            np.path,
            "/package[1]/metadata[1]/dc:contributor[1]/@opf:role"
        );
        assert_eq!(
            np.namespaces.get("opf"),
            Some(&"http://www.idpf.org/2007/opf".to_string())
        );
    }

    #[test]
    fn unprefixed_attribute_is_bare_with_no_binding() {
        // An unprefixed attribute is never in the default namespace, so `@id`
        // needs (and records) no binding of its own.
        let doc = roxmltree::Document::parse(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body id="x"/></html>"#,
        )
        .unwrap();
        let body = find(&doc, "body");
        let id = body.attributes().find(|a| a.name() == "id").unwrap();
        let np = node_path_attr(body, id);
        assert_eq!(np.path, "/html[1]/body[1]/@id");
        assert!(!np.namespaces.contains_key("id"));
    }

    #[test]
    fn sibling_index_counts_only_same_name_elements() {
        // `col` amid other element siblings: index counts cols only, 1-based.
        let doc = roxmltree::Document::parse(
            r#"<t xmlns="http://www.w3.org/1999/xhtml"><caption/><col/><col/><col/></t>"#,
        )
        .unwrap();
        let third_col = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "col")
            .nth(2)
            .unwrap();
        assert_eq!(node_path(third_col).path, "/t[1]/col[3]");
    }

    #[test]
    fn attr_no_ns_never_matches_a_namespaced_sibling() {
        // The exact confusion roxmltree 0.21 introduced: an element carrying
        // BOTH `lang` and `xml:lang`. `attr_no_ns` must see only the plain one,
        // on every roxmltree version.
        let doc = roxmltree::Document::parse(
            r#"<r xmlns:xml="http://www.w3.org/XML/1998/namespace" lang="en" xml:lang="fr"/>"#,
        )
        .unwrap();
        let r = doc.root_element();
        assert_eq!(r.attr_no_ns("lang"), Some("en"));
        // And when ONLY the namespaced one is present, it must find nothing.
        let doc2 = roxmltree::Document::parse(r#"<r xml:lang="fr"/>"#).unwrap();
        assert_eq!(doc2.root_element().attr_no_ns("lang"), None);
        assert!(!doc2.root_element().has_attr_no_ns("lang"));
    }
}
