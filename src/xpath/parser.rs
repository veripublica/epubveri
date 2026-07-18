//! Recursive-descent parser for the XPath 1.0 core subset, over the token
//! stream from `lexer.rs`, producing the `ast.rs` types.
//!
//! Grammar (lowest to highest precedence, standard XPath 1.0):
//! `Or > And > Equality > Relational > Additive > Multiplicative > Unary(-)
//! > Union(|) > Path`. `and`/`or`/`div`/`mod` are plain `Ident` tokens
//! lexically (see `lexer.rs`'s docs) — each precedence level checks for its
//! own keyword(s) by string value, exactly like it checks for `Token::Plus`
//! etc.
//!
//! `*`/keyword-vs-name ambiguity (e.g. is `div` the operator or an element
//! named "div"?) is resolved the same way real XPath resolves it: purely by
//! *grammar position*. By the time control reaches `parse_path_expr`, every
//! higher-precedence operator has already had first refusal at consuming
//! `and`/`or`/`div`/`mod`/`*`, so any of those tokens still present at the
//! step-parsing level can only mean a name/wildcard — no lookback needed.

use super::ast::{Axis, BinOp, Expr, NameTest, Path, PathStart, Step};
use super::lexer::{Lexer, Token};

pub fn parse(input: &str) -> Result<Expr, String> {
    let tokens = Lexer::new(input).tokenize();
    let mut p = Parser { tokens, pos: 0 };
    let e = p.parse_or_expr()?;
    if p.pos != p.tokens.len() {
        return Err(format!(
            "unexpected trailing input at token {:?}",
            p.tokens.get(p.pos)
        ));
    }
    Ok(e)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }
    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn eat_ident(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Token::Ident(i)) if i == s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and_expr()?;
        while self.eat_ident("or") {
            let right = self.parse_and_expr()?;
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality_expr()?;
        while self.eat_ident("and") {
            let right = self.parse_equality_expr()?;
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational_expr()?;
        loop {
            let op = if self.eat(&Token::Eq) {
                BinOp::Eq
            } else if self.eat(&Token::Ne) {
                BinOp::Ne
            } else {
                break;
            };
            let right = self.parse_relational_expr()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_relational_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive_expr()?;
        loop {
            let op = if self.eat(&Token::Le) {
                BinOp::Le
            } else if self.eat(&Token::Ge) {
                BinOp::Ge
            } else if self.eat(&Token::Lt) {
                BinOp::Lt
            } else if self.eat(&Token::Gt) {
                BinOp::Gt
            } else {
                break;
            };
            let right = self.parse_additive_expr()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative_expr()?;
        loop {
            let op = if self.eat(&Token::Plus) {
                BinOp::Add
            } else if self.eat(&Token::Minus) {
                BinOp::Sub
            } else {
                break;
            };
            let right = self.parse_multiplicative_expr()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary_expr()?;
        loop {
            let op = if self.eat(&Token::Star) {
                BinOp::Mul
            } else if self.eat_ident("div") {
                BinOp::Div
            } else if self.eat_ident("mod") {
                BinOp::Mod
            } else {
                break;
            };
            let right = self.parse_unary_expr()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Minus) {
            Ok(Expr::Neg(Box::new(self.parse_unary_expr()?)))
        } else {
            self.parse_union_expr()
        }
    }

    fn parse_union_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_path_expr()?;
        while self.eat(&Token::Pipe) {
            let right = self.parse_path_expr()?;
            left = Expr::Union(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// True if the *current* token can only begin a location-path `Step`
    /// (as opposed to a `PrimaryExpr` like a function call or variable).
    /// The one genuine ambiguity is a bare `Ident`: `count(` is a function
    /// call, `foo` (not followed by `(`) is a name test.
    fn looks_like_step_start(&self) -> bool {
        match self.peek() {
            Some(Token::At | Token::Dot | Token::DotDot | Token::Star) => true,
            Some(Token::Ident(_)) => !matches!(self.peek_at(1), Some(Token::LParen)),
            _ => false,
        }
    }

    fn parse_path_expr(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::SlashSlash) {
            let mut steps = vec![desc_or_self_step()];
            steps.push(self.parse_step()?);
            steps.extend(self.parse_more_steps()?);
            return Ok(Expr::Path(Path {
                start: PathStart::Root,
                steps,
            }));
        }
        if self.eat(&Token::Slash) {
            let steps = if self.looks_like_step_start() {
                let mut steps = vec![self.parse_step()?];
                steps.extend(self.parse_more_steps()?);
                steps
            } else {
                Vec::new()
            };
            return Ok(Expr::Path(Path {
                start: PathStart::Root,
                steps,
            }));
        }
        if self.looks_like_step_start() {
            let mut steps = vec![self.parse_step()?];
            steps.extend(self.parse_more_steps()?);
            return Ok(Expr::Path(Path {
                start: PathStart::Relative,
                steps,
            }));
        }

        // FilterExpr: PrimaryExpr Predicate*, optionally continued as a path.
        let primary = self.parse_primary_expr()?;
        let filter_predicates = self.parse_predicates()?;
        let mut steps = Vec::new();
        if !filter_predicates.is_empty() {
            steps.push(Step {
                axis: Axis::SelfAxis,
                test: NameTest::Any,
                predicates: filter_predicates,
            });
        }
        steps.extend(self.parse_more_steps()?);
        if steps.is_empty() {
            // No predicates and no further path continuation: just the bare
            // primary expression (a function call, literal, variable, or
            // parenthesized expression) — don't wrap it in a no-op Path.
            Ok(primary)
        } else {
            Ok(Expr::Path(Path {
                start: PathStart::Expr(Box::new(primary)),
                steps,
            }))
        }
    }

    fn parse_more_steps(&mut self) -> Result<Vec<Step>, String> {
        let mut steps = Vec::new();
        loop {
            if self.eat(&Token::SlashSlash) {
                steps.push(desc_or_self_step());
                steps.push(self.parse_step()?);
            } else if self.eat(&Token::Slash) {
                steps.push(self.parse_step()?);
            } else {
                break;
            }
        }
        Ok(steps)
    }

    fn parse_step(&mut self) -> Result<Step, String> {
        if self.eat(&Token::DotDot) {
            return Ok(Step {
                axis: Axis::Parent,
                test: NameTest::Any,
                predicates: self.parse_predicates()?,
            });
        }
        if self.eat(&Token::Dot) {
            return Ok(Step {
                axis: Axis::SelfAxis,
                test: NameTest::Any,
                predicates: self.parse_predicates()?,
            });
        }
        let axis = if self.eat(&Token::At) {
            Axis::Attribute
        } else if let Some(Token::Ident(name)) = self.peek().cloned() {
            if self.peek_at(1) == Some(&Token::ColonColon) {
                self.pos += 2;
                match name.as_str() {
                    "child" => Axis::Child,
                    "attribute" => Axis::Attribute,
                    "descendant" => Axis::Descendant,
                    "descendant-or-self" => Axis::DescendantOrSelf,
                    "parent" => Axis::Parent,
                    "ancestor" => Axis::Ancestor,
                    "ancestor-or-self" => Axis::AncestorOrSelf,
                    "preceding-sibling" => Axis::PrecedingSibling,
                    "self" => Axis::SelfAxis,
                    other => return Err(format!("unsupported XPath axis '{other}'")),
                }
            } else {
                Axis::Child
            }
        } else {
            Axis::Child
        };
        let test = if self.eat(&Token::Star) {
            NameTest::Any
        } else {
            match self.advance() {
                Some(Token::Ident(name)) => NameTest::Name(name),
                other => return Err(format!("expected a node test, found {other:?}")),
            }
        };
        let predicates = self.parse_predicates()?;
        Ok(Step {
            axis,
            test,
            predicates,
        })
    }

    fn parse_predicates(&mut self) -> Result<Vec<Expr>, String> {
        let mut preds = Vec::new();
        while self.eat(&Token::LBracket) {
            preds.push(self.parse_or_expr()?);
            if !self.eat(&Token::RBracket) {
                return Err("expected ']' to close a predicate".to_string());
            }
        }
        Ok(preds)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Some(Token::Variable(name)) => Ok(Expr::Variable(name)),
            Some(Token::LParen) => {
                let e = self.parse_or_expr()?;
                if !self.eat(&Token::RParen) {
                    return Err("expected ')'".to_string());
                }
                Ok(e)
            }
            Some(Token::Str(s)) => Ok(Expr::Str(s)),
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::Ident(name)) if self.eat(&Token::LParen) => {
                let mut args = Vec::new();
                if !self.eat(&Token::RParen) {
                    loop {
                        args.push(self.parse_or_expr()?);
                        if self.eat(&Token::Comma) {
                            continue;
                        }
                        break;
                    }
                    if !self.eat(&Token::RParen) {
                        return Err("expected ')' after function arguments".to_string());
                    }
                }
                Ok(Expr::Call(name, args))
            }
            other => Err(format!("unexpected token {other:?} in expression")),
        }
    }
}

fn desc_or_self_step() -> Step {
    Step {
        axis: Axis::DescendantOrSelf,
        test: NameTest::Any,
        predicates: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(e: &Expr) -> &Path {
        match e {
            Expr::Path(p) => p,
            other => panic!("expected a Path expr, got {other:?}"),
        }
    }

    #[test]
    fn relative_path_with_predicate() {
        let e = parse("*[@id]").unwrap();
        let p = path(&e);
        assert_eq!(p.start, PathStart::Relative);
        assert_eq!(p.steps.len(), 1);
        assert_eq!(p.steps[0].test, NameTest::Any);
        assert_eq!(p.steps[0].predicates.len(), 1);
    }

    #[test]
    fn absolute_double_slash() {
        let e = parse("//opf:package").unwrap();
        let p = path(&e);
        assert_eq!(p.start, PathStart::Root);
        assert_eq!(p.steps.len(), 2);
        assert_eq!(p.steps[0].axis, Axis::DescendantOrSelf);
        assert_eq!(p.steps[1].test, NameTest::Name("opf:package".into()));
    }

    #[test]
    fn ancestor_axis() {
        let e = parse("ancestor::opf:collection").unwrap();
        let p = path(&e);
        assert_eq!(p.steps[0].axis, Axis::Ancestor);
        assert_eq!(p.steps[0].test, NameTest::Name("opf:collection".into()));
    }

    #[test]
    fn function_call_is_not_a_step() {
        let e = parse("count(@id)").unwrap();
        match e {
            Expr::Call(name, args) => {
                assert_eq!(name, "count");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected a Call, got {other:?}"),
        }
    }

    #[test]
    fn variable_with_predicate_and_current() {
        // the exact shape id-uniqueness needs.
        let e = parse("$id-set[normalize-space(@id) = normalize-space(current()/@id)]").unwrap();
        let p = path(&e);
        assert_eq!(
            p.start,
            PathStart::Expr(Box::new(Expr::Variable("id-set".into())))
        );
        assert_eq!(p.steps.len(), 1);
        assert_eq!(p.steps[0].axis, Axis::SelfAxis);
        assert_eq!(p.steps[0].predicates.len(), 1);
    }

    #[test]
    fn boolean_and_arithmetic_precedence() {
        // "count(x) = 1 and not(@y)" should parse as (count(x)=1) and (not(@y))
        let e = parse("count(x) = 1 and not(@y)").unwrap();
        match e {
            Expr::BinOp(_, BinOp::And, _) => {}
            other => panic!("expected top-level 'and', got {other:?}"),
        }
    }

    #[test]
    fn parenthesized_and_arithmetic() {
        let e = parse("(1 + 2) * 3").unwrap();
        match e {
            Expr::BinOp(l, BinOp::Mul, r) => {
                assert!(matches!(*l, Expr::BinOp(_, BinOp::Add, _)));
                assert_eq!(*r, Expr::Number(3.0));
            }
            other => panic!("expected multiplication at top level, got {other:?}"),
        }
    }

    #[test]
    fn attribute_and_parent_axis_abbreviations() {
        let e = parse("../h:meta[@charset]").unwrap();
        let p = path(&e);
        assert_eq!(p.steps[0].axis, Axis::Parent);
        assert_eq!(p.steps[1].test, NameTest::Name("h:meta".into()));
        assert_eq!(p.steps[1].predicates.len(), 1);
    }
}
