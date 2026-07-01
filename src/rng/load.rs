//! Load a RELAX NG schema in the **XML syntax** into a [`Grammar`].
//!
//! Scope: the common subset — `grammar`/`start`/`define`/`ref`, `element`/
//! `attribute` (name attr or name-class child), `group`/`choice`/`interleave`/
//! `optional`/`zeroOrMore`/`oneOrMore`/`list`/`mixed`, `empty`/`text`/
//! `notAllowed`/`value`/`data`, with `ns` and `datatypeLibrary` inheritance and
//! prefix resolution. `ref`s are kept as `Pattern::Ref` indices into the
//! grammar's definitions, so **recursive grammars are supported** (no inlining).
//! Not yet: `<include>`/`<externalRef>`, `combine`, `<param>` facets.

use std::collections::HashMap;

use roxmltree::Node;

use super::derive::Grammar;
use super::pattern::*;

const RNG_NS: &str = "http://relaxng.org/ns/structure/1.0";

fn is_rng(n: Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(RNG_NS)
}
fn lname<'a>(n: Node<'a, 'a>) -> &'a str {
    n.tag_name().name()
}
fn rng_children<'a>(n: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    n.children().filter(|c| is_rng(*c))
}

struct Loader {
    /// definition name -> index into `Grammar::defs`
    index: HashMap<String, usize>,
}

/// Parse a RELAX NG (XML syntax) schema string into a [`Grammar`].
pub fn load(xml: &str) -> Result<Grammar, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let ns0 = root.attribute("ns").unwrap_or("");
    let dt0 = root.attribute("datatypeLibrary").unwrap_or("");

    if is_rng(root) && lname(root) == "grammar" {
        let def_nodes: Vec<Node> = rng_children(root)
            .filter(|c| lname(*c) == "define")
            .collect();
        let mut index = HashMap::new();
        for (i, d) in def_nodes.iter().enumerate() {
            if let Some(name) = d.attribute("name") {
                index.insert(name.to_string(), i);
            }
        }
        let l = Loader { index };

        let start = rng_children(root)
            .find(|c| lname(*c) == "start")
            .ok_or("<grammar> has no <start>")?;
        let start_pat = l.group_nodes(&rng_children(start).collect::<Vec<_>>(), ns0, dt0)?;

        let mut defs = Vec::with_capacity(def_nodes.len());
        for d in &def_nodes {
            defs.push(l.group_nodes(&rng_children(*d).collect::<Vec<_>>(), ns0, dt0)?);
        }
        Ok(Grammar {
            start: start_pat,
            defs,
        })
    } else {
        let l = Loader {
            index: HashMap::new(),
        };
        Ok(Grammar::single(l.build(root, ns0, dt0)?))
    }
}

impl Loader {
    fn build<'a>(&self, node: Node<'a, 'a>, ns: &str, dtlib: &str) -> Result<Pat, String> {
        let ns = node.attribute("ns").unwrap_or(ns);
        let dtlib = node.attribute("datatypeLibrary").unwrap_or(dtlib);
        match lname(node) {
            "element" => {
                let kids: Vec<_> = rng_children(node).collect();
                let (nc, content) = if let Some(nm) = node.attribute("name") {
                    (
                        self.name_from_str(node, nm, false, ns)?,
                        self.group_nodes(&kids, ns, dtlib)?,
                    )
                } else {
                    let (first, rest) = kids
                        .split_first()
                        .ok_or("<element> missing name / name-class")?;
                    (
                        self.name_class(*first, ns)?,
                        self.group_nodes(rest, ns, dtlib)?,
                    )
                };
                Ok(element(nc, content))
            }
            "attribute" => {
                let kids: Vec<_> = rng_children(node).collect();
                let (nc, rest): (NameClass, &[Node]) = if let Some(nm) = node.attribute("name") {
                    (self.name_from_str(node, nm, true, ns)?, &kids)
                } else {
                    let (first, rest) = kids.split_first().ok_or("<attribute> missing name")?;
                    (self.name_class(*first, ns)?, rest)
                };
                let content = if rest.is_empty() {
                    text()
                } else {
                    self.group_nodes(rest, ns, dtlib)?
                };
                Ok(attribute(nc, content))
            }
            "group" => self.group_children(node, ns, dtlib),
            "interleave" => self.fold_children(node, ns, dtlib, empty(), interleave),
            "choice" => self.fold_children(node, ns, dtlib, not_allowed(), choice),
            "optional" => Ok(optional(self.group_children(node, ns, dtlib)?)),
            "zeroOrMore" => Ok(zero_or_more(self.group_children(node, ns, dtlib)?)),
            "oneOrMore" => Ok(one_or_more(self.group_children(node, ns, dtlib)?)),
            "list" => Ok(list(self.group_children(node, ns, dtlib)?)),
            "mixed" => Ok(interleave(text(), self.group_children(node, ns, dtlib)?)),
            "empty" => Ok(empty()),
            "text" => Ok(text()),
            "notAllowed" => Ok(not_allowed()),
            "value" => Ok(value(
                Datatype::from(dtlib, node.attribute("type").unwrap_or("token")),
                node.text().unwrap_or("").to_string(),
            )),
            "data" => Ok(data(Datatype::from(
                dtlib,
                node.attribute("type").unwrap_or("token"),
            ))),
            "ref" => {
                let name = node.attribute("name").ok_or("<ref> without name")?;
                let i = *self
                    .index
                    .get(name)
                    .ok_or_else(|| format!("ref to undefined '{name}'"))?;
                Ok(ref_(i))
            }
            other => Err(format!("unsupported RNG element <{other}>")),
        }
    }

    fn group_children<'a>(&self, node: Node<'a, 'a>, ns: &str, dtlib: &str) -> Result<Pat, String> {
        self.group_nodes(&rng_children(node).collect::<Vec<_>>(), ns, dtlib)
    }

    fn group_nodes<'a>(
        &self,
        nodes: &[Node<'a, 'a>],
        ns: &str,
        dtlib: &str,
    ) -> Result<Pat, String> {
        let mut acc = empty();
        for n in nodes {
            acc = group(acc, self.build(*n, ns, dtlib)?);
        }
        Ok(acc)
    }

    fn fold_children<'a>(
        &self,
        node: Node<'a, 'a>,
        ns: &str,
        dtlib: &str,
        init: Pat,
        combine: fn(Pat, Pat) -> Pat,
    ) -> Result<Pat, String> {
        let mut acc = init;
        for n in rng_children(node) {
            acc = combine(acc, self.build(n, ns, dtlib)?);
        }
        Ok(acc)
    }

    fn name_from_str<'a>(
        &self,
        node: Node<'a, 'a>,
        name: &str,
        is_attr: bool,
        ns: &str,
    ) -> Result<NameClass, String> {
        if let Some((pfx, local)) = name.split_once(':') {
            let uri = node
                .lookup_namespace_uri(Some(pfx))
                .ok_or_else(|| format!("unknown namespace prefix '{pfx}'"))?;
            Ok(qname(uri, local))
        } else {
            let ns = if is_attr { "" } else { ns };
            Ok(qname(ns, name))
        }
    }

    fn name_class<'a>(&self, node: Node<'a, 'a>, ns: &str) -> Result<NameClass, String> {
        let ns = node.attribute("ns").unwrap_or(ns);
        match lname(node) {
            "name" => self.name_from_str(node, node.text().unwrap_or("").trim(), false, ns),
            "anyName" => Ok(match self.except_of(node, ns)? {
                Some(e) => NameClass::AnyNameExcept(Box::new(e)),
                None => NameClass::AnyName,
            }),
            "nsName" => Ok(match self.except_of(node, ns)? {
                Some(e) => NameClass::NsNameExcept {
                    ns: ns.to_string(),
                    except: Box::new(e),
                },
                None => NameClass::NsName { ns: ns.to_string() },
            }),
            "choice" => {
                let mut it = rng_children(node);
                let first = it.next().ok_or("empty name-class <choice>")?;
                let mut acc = self.name_class(first, ns)?;
                for c in it {
                    acc = NameClass::Choice(Box::new(acc), Box::new(self.name_class(c, ns)?));
                }
                Ok(acc)
            }
            other => Err(format!("unsupported name class <{other}>")),
        }
    }

    fn except_of<'a>(&self, node: Node<'a, 'a>, ns: &str) -> Result<Option<NameClass>, String> {
        let Some(exc) = rng_children(node).find(|c| lname(*c) == "except") else {
            return Ok(None);
        };
        let mut it = rng_children(exc);
        let first = it.next().ok_or("empty <except>")?;
        let mut acc = self.name_class(first, ns)?;
        for c in it {
            acc = NameClass::Choice(Box::new(acc), Box::new(self.name_class(c, ns)?));
        }
        Ok(Some(acc))
    }
}

#[cfg(test)]
mod tests {
    use super::super::validate_xml;
    use super::*;

    const NOTE_RNG: &str = concat!(
        "<element name=\"note\" xmlns=\"http://relaxng.org/ns/structure/1.0\">",
        "<element name=\"to\"><text/></element>",
        "<optional><element name=\"from\"><text/></element></optional>",
        "</element>"
    );

    #[test]
    fn loads_toy_grammar() {
        let g = load(NOTE_RNG).unwrap();
        assert!(validate_xml(&g, "<note><to>x</to></note>").unwrap());
        assert!(validate_xml(&g, "<note><to>x</to><from>y</from></note>").unwrap());
        assert!(!validate_xml(&g, "<note></note>").unwrap());
        assert!(!validate_xml(&g, "<note><from>y</from></note>").unwrap());
    }

    const CONTAINER_RNG: &str = concat!(
        "<grammar xmlns=\"http://relaxng.org/ns/structure/1.0\" ",
        "ns=\"urn:oasis:names:tc:opendocument:xmlns:container\" ",
        "datatypeLibrary=\"http://www.w3.org/2001/XMLSchema-datatypes\">",
        "<start><element name=\"container\">",
        "<attribute name=\"version\"><value type=\"token\">1.0</value></attribute>",
        "<element name=\"rootfiles\"><oneOrMore>",
        "<element name=\"rootfile\">",
        "<attribute name=\"full-path\"><data type=\"anyURI\"/></attribute>",
        "<attribute name=\"media-type\"><data type=\"string\"/></attribute>",
        "</element></oneOrMore></element>",
        "</element></start></grammar>"
    );

    fn cvalid() -> String {
        concat!(
            "<container version=\"1.0\" ",
            "xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">",
            "<rootfiles><rootfile full-path=\"OEBPS/content.opf\" ",
            "media-type=\"application/oebps-package+xml\"/></rootfiles></container>"
        )
        .to_string()
    }

    #[test]
    fn loads_container_grammar_from_rng() {
        let g = load(CONTAINER_RNG).unwrap();
        assert!(validate_xml(&g, &cvalid()).unwrap());
        assert!(
            !validate_xml(&g, &cvalid().replace("version=\"1.0\"", "version=\"2.0\"")).unwrap()
        );
        assert!(!validate_xml(
            &g,
            &cvalid().replace(" media-type=\"application/oebps-package+xml\"", "")
        )
        .unwrap());
        assert!(!validate_xml(
            &g,
            &cvalid().replace("<rootfiles>", "<rootfiles bogus=\"x\">")
        )
        .unwrap());
    }

    #[test]
    fn datatypes_through_loader() {
        let rng = concat!(
            "<element name=\"meta\" xmlns=\"http://relaxng.org/ns/structure/1.0\" ",
            "datatypeLibrary=\"http://www.w3.org/2001/XMLSchema-datatypes\">",
            "<attribute name=\"lang\"><data type=\"language\"/></attribute>",
            "<data type=\"nonNegativeInteger\"/>",
            "</element>"
        );
        let g = load(rng).unwrap();
        assert!(validate_xml(&g, "<meta lang=\"en-US\">42</meta>").unwrap());
        assert!(!validate_xml(&g, "<meta lang=\"en_US\">42</meta>").unwrap());
        assert!(!validate_xml(&g, "<meta lang=\"en\">-3</meta>").unwrap());
    }

    const SECTION_RNG: &str = concat!(
        "<grammar xmlns=\"http://relaxng.org/ns/structure/1.0\">",
        "<start><ref name=\"section\"/></start>",
        "<define name=\"section\"><element name=\"section\">",
        "<optional><text/></optional><zeroOrMore><ref name=\"section\"/></zeroOrMore>",
        "</element></define></grammar>"
    );

    #[test]
    fn loads_recursive_grammar() {
        let g = load(SECTION_RNG).unwrap();
        assert!(validate_xml(&g, "<section/>").unwrap());
        assert!(validate_xml(&g, "<section><section/></section>").unwrap());
        assert!(validate_xml(
            &g,
            "<section><section><section/></section><section/></section>"
        )
        .unwrap());
        assert!(!validate_xml(&g, "<section><x/></section>").unwrap());
    }
}
