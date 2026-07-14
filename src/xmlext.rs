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

#[cfg(test)]
mod tests {
    use super::NodeExt;

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
