//! A small, real XPath **1.0** engine (not the full spec — see the module
//! docs in `parser.rs` and the exclusions noted in `eval.rs`), built to
//! evaluate Schematron `context`/`test`/`value` expressions against a
//! `roxmltree` document. Not a general XPath 1.0 implementation project —
//! scoped to exactly what epubcheck's real Schematron rules need, minus
//! `matches()`/`tokenize()`/`resolve-uri()` (regex/URI-resolution engines,
//! deferred).

pub mod ast;
pub mod eval;
pub mod lexer;
pub mod parser;

pub use ast::Expr;
pub use eval::{Env, NodeRef, Value, eval, eval_boolean};
pub use parser::parse;
