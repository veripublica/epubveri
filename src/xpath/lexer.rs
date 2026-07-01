//! A lexer for the XPath 1.0 expression grammar subset this engine supports.
//!
//! Deliberately simple: every name (including keywords like `and`/`or`/
//! `div`/`mod`/`not` and axis names like `ancestor`) is emitted as a single
//! `Ident` token, and `*` is always `Star`. XPath's real grammar disambiguates
//! "is this `*`/`and`/... an operator or a name-test/wildcard" purely by
//! *grammar position* (which recursive-descent production is running), not
//! by lookback — so the parser resolves this structurally; the lexer doesn't
//! need to track state to do it.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Variable(String),
    Str(String),
    Number(f64),
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Pipe,
    Slash,
    SlashSlash,
    Dot,
    DotDot,
    At,
    ColonColon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer { input, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    fn peek2(&self) -> Option<char> {
        self.input[self.pos..].chars().nth(1)
    }
    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn read_while(&mut self, pred: impl Fn(char) -> bool) -> &'a str {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if pred(c)) {
            self.advance();
        }
        &self.input[start..self.pos]
    }

    fn read_ident_like(&mut self) -> String {
        // NCName-ish: letters/digits/_/-/. (XPath NCNames also allow '-' and
        // '.' after the first character; we're lenient rather than exact).
        let s = self.read_while(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
        s.to_string()
    }

    pub fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        let c = self.peek()?;
        Some(match c {
            '$' => {
                self.advance();
                Token::Variable(self.read_ident_like())
            }
            '"' | '\'' => {
                let quote = c;
                self.advance();
                let start = self.pos;
                while matches!(self.peek(), Some(ch) if ch != quote) {
                    self.advance();
                }
                let s = self.input[start..self.pos].to_string();
                self.advance(); // closing quote
                Token::Str(s)
            }
            c if c.is_ascii_digit() => {
                let start = self.pos;
                self.read_while(|c| c.is_ascii_digit());
                if self.peek() == Some('.') {
                    self.advance();
                    self.read_while(|c| c.is_ascii_digit());
                }
                let repr = &self.input[start..self.pos];
                Token::Number(repr.parse().unwrap_or(0.0))
            }
            '.' if self.peek2().map(|c| c.is_ascii_digit()).unwrap_or(false) => {
                let start = self.pos;
                self.advance();
                self.read_while(|c| c.is_ascii_digit());
                let repr = &self.input[start..self.pos];
                Token::Number(repr.parse().unwrap_or(0.0))
            }
            '.' if self.peek2() == Some('.') => {
                self.pos += 2;
                Token::DotDot
            }
            '.' => {
                self.advance();
                Token::Dot
            }
            '/' if self.peek2() == Some('/') => {
                self.pos += 2;
                Token::SlashSlash
            }
            '/' => {
                self.advance();
                Token::Slash
            }
            ':' if self.peek2() == Some(':') => {
                self.pos += 2;
                Token::ColonColon
            }
            '@' => {
                self.advance();
                Token::At
            }
            '(' => {
                self.advance();
                Token::LParen
            }
            ')' => {
                self.advance();
                Token::RParen
            }
            '[' => {
                self.advance();
                Token::LBracket
            }
            ']' => {
                self.advance();
                Token::RBracket
            }
            ',' => {
                self.advance();
                Token::Comma
            }
            '|' => {
                self.advance();
                Token::Pipe
            }
            '+' => {
                self.advance();
                Token::Plus
            }
            '-' => {
                self.advance();
                Token::Minus
            }
            '*' => {
                self.advance();
                Token::Star
            }
            '=' => {
                self.advance();
                Token::Eq
            }
            '!' if self.peek2() == Some('=') => {
                self.pos += 2;
                Token::Ne
            }
            '<' if self.peek2() == Some('=') => {
                self.pos += 2;
                Token::Le
            }
            '<' => {
                self.advance();
                Token::Lt
            }
            '>' if self.peek2() == Some('=') => {
                self.pos += 2;
                Token::Ge
            }
            '>' => {
                self.advance();
                Token::Gt
            }
            c if c.is_alphabetic() || c == '_' => {
                // NCName, possibly qualified (prefix:local) — a single
                // colon (not `::`) is part of the qname, handled here so
                // `opf:package` lexes as one Ident("opf:package").
                let mut name = self.read_ident_like();
                if self.peek() == Some(':') && self.peek2() != Some(':') {
                    self.advance();
                    name.push(':');
                    name.push_str(&self.read_ident_like());
                }
                Token::Ident(name)
            }
            other => {
                // Unrecognized character: skip it rather than fail — this
                // engine never panics on malformed input (same non-fatal
                // philosophy as the rest of epubveri's parsers); an
                // expression that can't be fully tokenized will simply fail
                // to parse a complete AST later, which the caller treats as
                // "condition doesn't hold" rather than crashing.
                self.advance();
                Token::Ident(other.to_string())
            }
        })
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut out = Vec::new();
        while let Some(t) = self.next_token() {
            out.push(t);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_path() {
        assert_eq!(
            Lexer::new("opf:package/opf:metadata").tokenize(),
            vec![
                Token::Ident("opf:package".into()),
                Token::Slash,
                Token::Ident("opf:metadata".into()),
            ]
        );
    }

    #[test]
    fn predicate_and_attribute() {
        assert_eq!(
            Lexer::new("*[@id]").tokenize(),
            vec![
                Token::Star,
                Token::LBracket,
                Token::At,
                Token::Ident("id".into()),
                Token::RBracket
            ]
        );
    }

    #[test]
    fn variable_and_function_call() {
        assert_eq!(
            Lexer::new("count($id-set) = 1").tokenize(),
            vec![
                Token::Ident("count".into()),
                Token::LParen,
                Token::Variable("id-set".into()),
                Token::RParen,
                Token::Eq,
                Token::Number(1.0),
            ]
        );
    }

    #[test]
    fn axis_and_descendant() {
        assert_eq!(
            Lexer::new("ancestor::opf:collection").tokenize(),
            vec![
                Token::Ident("ancestor".into()),
                Token::ColonColon,
                Token::Ident("opf:collection".into())
            ]
        );
        assert_eq!(
            Lexer::new("//*").tokenize(),
            vec![Token::SlashSlash, Token::Star]
        );
    }

    #[test]
    fn string_and_number_literals() {
        assert_eq!(
            Lexer::new(r#"'a "b" c' "d 'e' f" 3.14 -2"#).tokenize(),
            vec![
                Token::Str("a \"b\" c".into()),
                Token::Str("d 'e' f".into()),
                Token::Number(3.14),
                Token::Minus,
                Token::Number(2.0),
            ]
        );
    }

    #[test]
    fn operators_and_relational() {
        assert_eq!(
            Lexer::new("!= <= >= < >").tokenize(),
            vec![Token::Ne, Token::Le, Token::Ge, Token::Lt, Token::Gt]
        );
    }
}
