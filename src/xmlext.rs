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

use std::collections::{BTreeMap, BTreeSet};

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

/// The attribute with local name `name` and **no namespace** as a whole
/// [`roxmltree::Attribute`] (not just its value, like [`NodeExt::attr_no_ns`]) —
/// for passing to [`Report::push_node_attr`](crate::report::Report::push_node_attr)
/// so a finding's `element_path` can pin `@name` (issue #18).
pub(crate) fn attr_no_ns_node<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Attribute<'a, 'input>> {
    node.attributes()
        .find(|a| a.namespace().is_none() && a.name() == name)
}

/// Like [`attr_no_ns_node`], but for the attribute with local name `name` in
/// namespace `uri` (e.g. `epub:prefix` in the OPS namespace) — the whole
/// [`roxmltree::Attribute`], for `@prefix:name`-targeted findings (issue #18).
pub(crate) fn attr_ns_node<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    uri: &str,
    name: &str,
) -> Option<roxmltree::Attribute<'a, 'input>> {
    node.attributes()
        .find(|a| a.namespace() == Some(uri) && a.name() == name)
}

/// A machine-resolvable, XPath/lxml-ElementPath-style location for a
/// node-anchored finding (issue #18), plus the namespace bindings needed to
/// resolve it against a parsed tree.
///
/// The path is rooted with 1-based sibling indices, e.g.
/// `/opf:package[1]/opf:metadata[1]/dc:contributor[1]`, optionally ending in an
/// `/@prefix:name` attribute (or a bare `/@name` for a namespace-less one).
///
/// **Every namespaced name carries a non-empty prefix**, and every prefix is
/// bound in [`namespaces`](Self::namespaces) — there is deliberately no
/// empty-string / default-namespace entry. This is because the dominant XPath
/// engine, libxml2 (behind `lxml`), implements XPath 1.0, which has *no* default
/// namespace: `xpath("/package")` matches nothing against a default-namespaced
/// document, and `""`/`None` cannot be bound as a namespace there. So a
/// default-namespaced element is given a synthesized prefix (a readable
/// well-known one for the common EPUB namespaces, else a generated `ns…`), and
/// the map binds it — making the path resolvable as-is (jenstroeger, #18).
#[derive(Debug, Clone)]
pub struct NodePath {
    pub path: String,
    /// Prefix -> namespace-URI bindings used by `path`. Never contains an
    /// empty-string key. Empty when the path touches no namespaced name.
    pub namespaces: BTreeMap<String, String>,
}

/// Per-path prefix allocator: assigns each distinct namespace URI a unique,
/// **non-empty** prefix, so the path is resolvable by an XPath 1.0 engine
/// (libxml2/lxml), which cannot bind a default namespace. Prefers the
/// source-authored prefix, then a readable well-known one, then a generated
/// `ns`/`ns1`/… — bumping with a numeric suffix on collision.
struct Prefixes {
    by_uri: BTreeMap<String, String>,
    used: BTreeSet<String>,
}

impl Prefixes {
    fn new() -> Self {
        Self {
            by_uri: BTreeMap::new(),
            used: BTreeSet::new(),
        }
    }

    fn get(&mut self, uri: &str, source: Option<&str>) -> String {
        if let Some(p) = self.by_uri.get(uri) {
            return p.clone();
        }
        let base = source
            .map(str::to_string)
            .or_else(|| well_known_prefix(uri).map(str::to_string))
            .unwrap_or_else(|| "ns".to_string());
        let mut p = base.clone();
        let mut n = 0u32;
        while self.used.contains(&p) {
            n += 1;
            p = format!("{base}{n}");
        }
        self.used.insert(p.clone());
        self.by_uri.insert(uri.to_string(), p.clone());
        p
    }

    /// The `prefix -> URI` map for the [`NodePath`] (inverse of `by_uri`; the
    /// allocator guarantees prefixes are unique, so this is a clean bijection).
    fn into_map(self) -> BTreeMap<String, String> {
        self.by_uri
            .into_iter()
            .map(|(uri, prefix)| (prefix, uri))
            .collect()
    }
}

/// A short, conventional prefix for a well-known EPUB namespace, so synthesized
/// prefixes read naturally (`/opf:package`, not `/ns:package`). `h` for XHTML
/// matches epubcheck's own schematron convention.
fn well_known_prefix(uri: &str) -> Option<&'static str> {
    Some(match uri {
        "http://www.w3.org/1999/xhtml" => "h",
        "http://www.idpf.org/2007/opf" => "opf",
        "http://purl.org/dc/elements/1.1/" => "dc",
        "http://purl.org/dc/terms/" => "dcterms",
        "http://www.idpf.org/2007/ops" => "epub",
        "http://www.w3.org/2000/svg" => "svg",
        "http://www.w3.org/1998/Math/MathML" => "mathml",
        "http://www.w3.org/1999/xlink" => "xlink",
        "http://www.daisy.org/z3986/2005/ncx/" => "ncx",
        "http://www.w3.org/XML/1998/namespace" => "xml",
        _ => return None,
    })
}

/// Build the element path to `node` (issue #18). `node` is the element the
/// finding is anchored at; the resulting path targets that element.
pub(crate) fn node_path(node: roxmltree::Node) -> NodePath {
    let mut prefixes = Prefixes::new();
    let path = element_path_string(node, &mut prefixes);
    NodePath {
        path,
        namespaces: prefixes.into_map(),
    }
}

/// Like [`node_path`], but the finding is about a run of text rather than an
/// element; the path ends in a `/text()[n]` step addressing that run.
///
/// Without this a loose-text finding resolves to its *containing element* —
/// `element_path_string` walks ancestors and keeps only elements, so a text
/// node's own step is dropped and the path silently points one level up. For
/// "there is text here that shouldn't be" that is the one thing the reader
/// needs and the one thing it didn't say.
///
/// `n` counts text nodes among the parent's children, 1-based, which is what
/// XPath's `text()[n]` selects.
pub(crate) fn node_path_text(text: roxmltree::Node) -> NodePath {
    let mut prefixes = Prefixes::new();
    let Some(parent) = text.parent().filter(|p| p.is_element()) else {
        // No element to hang it off - fall back rather than invent a path.
        return node_path(text);
    };
    let mut path = element_path_string(parent, &mut prefixes);
    let idx = parent
        .children()
        .filter(roxmltree::Node::is_text)
        .position(|n| n == text)
        .map_or(1, |i| i + 1);
    path.push_str(&format!("/text()[{idx}]"));
    NodePath {
        path,
        namespaces: prefixes.into_map(),
    }
}

/// Like [`node_path`], but the finding is about a specific attribute of `node`;
/// the path ends in an `/@prefix:name` (or bare `/@name`) step. Pass the
/// [`roxmltree::Attribute`] in hand so its namespace is resolved exactly, not
/// guessed from a local name.
pub(crate) fn node_path_attr(node: roxmltree::Node, attr: roxmltree::Attribute) -> NodePath {
    let mut prefixes = Prefixes::new();
    let mut path = element_path_string(node, &mut prefixes);
    // An unprefixed attribute is never in a namespace (the default namespace
    // never applies to attributes), so it's a bare `@name`. A namespaced
    // attribute (`opf:role`) gets a bound prefix like any other name.
    let step = match attr.namespace() {
        Some(uri) => {
            let p = prefixes.get(uri, prefixed_binding(node, uri));
            format!("@{p}:{}", attr.name())
        }
        None => format!("@{}", attr.name()),
    };
    path.push('/');
    path.push_str(&step);
    NodePath {
        path,
        namespaces: prefixes.into_map(),
    }
}

/// The rooted `/a[1]/b[2]/…` element path, allocating prefixes into `prefixes`.
fn element_path_string(node: roxmltree::Node, prefixes: &mut Prefixes) -> String {
    // `ancestors()` yields self first, then parents; the document root is not an
    // element, so filtering to elements drops it (no bogus leading segment).
    let mut segments: Vec<String> = node
        .ancestors()
        .filter(roxmltree::Node::is_element)
        .map(|el| element_segment(el, prefixes))
        .collect();
    segments.reverse();
    format!("/{}", segments.join("/"))
}

/// One `name[index]` path segment for an element, allocating a prefix for its
/// namespace if it has one.
fn element_segment(el: roxmltree::Node, prefixes: &mut Prefixes) -> String {
    let name = el.tag_name();
    let local = name.name();
    let qname = match name.namespace() {
        None => local.to_string(),
        Some(uri) => {
            // A default-namespaced element has no authored prefix (pass `None` so
            // a well-known/generated one is synthesized); a prefixed element
            // hints its source prefix.
            let source = if el.default_namespace() == Some(uri) {
                None
            } else {
                prefixed_binding(el, uri)
            };
            format!("{}:{local}", prefixes.get(uri, source))
        }
    };
    format!("{qname}[{}]", element_index(el))
}

/// The source prefix bound to `uri` in scope at `node` (never a same-URI default
/// binding, whose name is empty). Used as the allocator's preferred hint for a
/// name the document authored with a prefix.
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
    fn default_namespace_element_gets_a_synthesized_bound_prefix() {
        // XHTML: html/body/p all live in the *default* namespace. XPath 1.0
        // (libxml2/lxml) can't match a bare `/html` there and can't bind an
        // empty prefix, so each name gets a synthesized, bound prefix (`h` for
        // XHTML) — never an empty-string key (#18, jenstroeger).
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
        assert_eq!(np.path, "/h:html[1]/h:body[1]/h:p[2]");
        assert_eq!(
            np.namespaces.get("h"),
            Some(&"http://www.w3.org/1999/xhtml".to_string())
        );
        assert!(!np.namespaces.contains_key(""), "no empty-string key");
    }

    #[test]
    fn prefixed_and_default_names_each_get_a_bound_prefix() {
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
        // default-namespaced package/metadata -> synthesized `opf`; dc keeps its
        // source prefix.
        assert_eq!(np.path, "/opf:package[1]/opf:metadata[1]/dc:contributor[1]");
        assert_eq!(
            np.namespaces.get("dc"),
            Some(&"http://purl.org/dc/elements/1.1/".to_string())
        );
        assert_eq!(
            np.namespaces.get("opf"),
            Some(&"http://www.idpf.org/2007/opf".to_string())
        );
        assert!(!np.namespaces.contains_key(""));
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
            "/opf:package[1]/opf:metadata[1]/dc:contributor[1]/@opf:role"
        );
        assert_eq!(
            np.namespaces.get("opf"),
            Some(&"http://www.idpf.org/2007/opf".to_string())
        );
    }

    #[test]
    fn unprefixed_attribute_is_bare_with_no_binding() {
        // An unprefixed attribute is never in a namespace, so `@id` needs (and
        // records) no binding of its own — only the element names do.
        let doc = roxmltree::Document::parse(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body id="x"/></html>"#,
        )
        .unwrap();
        let body = find(&doc, "body");
        let id = body.attributes().find(|a| a.name() == "id").unwrap();
        let np = node_path_attr(body, id);
        assert_eq!(np.path, "/h:html[1]/h:body[1]/@id");
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
        assert_eq!(node_path(third_col).path, "/h:t[1]/h:col[3]");
    }

    #[test]
    fn attr_node_helpers_select_by_namespace() {
        let doc = roxmltree::Document::parse(
            r#"<r xmlns:epub="http://www.idpf.org/2007/ops" prefix="a" epub:prefix="b"/>"#,
        )
        .unwrap();
        let r = doc.root_element();
        // no-ns helper sees the plain attribute, not its namespaced sibling
        assert_eq!(super::attr_no_ns_node(r, "prefix").unwrap().value(), "a");
        // ns helper sees only the one in the given namespace
        assert_eq!(
            super::attr_ns_node(r, "http://www.idpf.org/2007/ops", "prefix")
                .unwrap()
                .value(),
            "b"
        );
        assert!(super::attr_ns_node(r, "http://example.org/other", "prefix").is_none());
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

#[cfg(test)]
mod text_path_tests {
    use super::*;

    /// A loose-text finding must address the text run, not resolve to the
    /// element that contains it - "there is text here that shouldn't be" is
    /// exactly the case where the element is not the answer.
    ///
    /// The index is XPath's: `text()[n]` counts text nodes among the
    /// parent's children, so the newline before `<p>` is `text()[1]` and the
    /// loose run after it is `text()[2]`. Verified against an independent
    /// parser's node ordering, not just eyeballed.
    #[test]
    fn text_path_addresses_the_run_and_counts_like_xpath() {
        let xml =
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>\n<p>ok</p>\nloose\n</body></html>";
        let d = crate::ocf::parse_xml(xml).unwrap();
        let body = d.descendants().find(|n| n.has_tag_name("body")).unwrap();
        let runs: Vec<_> = body.children().filter(|n| n.is_text()).collect();
        // Two runs, not three: everything after `</p>` is one text node.
        assert_eq!(runs.len(), 2, "got {runs:?}");

        let p = node_path_text(runs[1]);
        assert_eq!(p.path, "/h:html[1]/h:body[1]/text()[2]");
        assert_eq!(
            p.namespaces.get("h").map(String::as_str),
            Some(XHTML_NS_FOR_TEST)
        );

        // The run before `<p>` is a different one, and says so.
        assert_eq!(
            node_path_text(runs[0]).path,
            "/h:html[1]/h:body[1]/text()[1]"
        );
    }

    const XHTML_NS_FOR_TEST: &str = "http://www.w3.org/1999/xhtml";
}
