//! AST for the XPath 1.0 core subset. Abbreviated syntax (`//`, `..`, `@`)
//! is desugared straight into explicit axes at parse time (exactly how the
//! XPath spec itself defines their meaning), so the evaluator only ever
//! needs to handle one uniform `Step` shape.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Child,
    Attribute,
    /// Desugared target of the `//` abbreviation (`//foo` ==
    /// `/descendant-or-self::node()/child::foo`).
    DescendantOrSelf,
    Descendant,
    Parent,
    Ancestor,
    AncestorOrSelf,
    SelfAxis,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NameTest {
    /// `*` or the desugared `node()` test used for `//`/`.`.
    Any,
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub axis: Axis,
    pub test: NameTest,
    pub predicates: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathStart {
    /// Leading `/`.
    Root,
    /// No leading `/`: starts from the current context node(s).
    Relative,
    /// Starts from an arbitrary expression's result (e.g. `$var/...` or
    /// `current()/...`).
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub start: PathStart,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Path(Path),
    Variable(String),
    Str(String),
    Number(f64),
    /// Includes `not(...)` — XPath defines `not` as an ordinary function,
    /// not a grammar-level unary operator.
    Call(String, Vec<Expr>),
    /// Unary `-`, which *is* a grammar-level operator in XPath.
    Neg(Box<Expr>),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    /// `|` (node-set union).
    Union(Box<Expr>, Box<Expr>),
}
