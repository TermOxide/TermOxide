//! Parser for converting RSX macro tokens into the owned RSX AST.

use proc_macro2::{Delimiter, Ident, Literal, Span, TokenStream, TokenTree};

use crate::ast::{Attribute, AttributeValue, Lit, RsxExpr, RsxNode};

/// Parse RSX tokens into an [`RsxNode`] tree.
///
/// Syntax errors retain their source span for compiler diagnostics.
pub fn parse(input: TokenStream) -> Result<RsxNode, syn::Error> {
    let mut parser = Parser::new(input);
    let node = parser.parse_node()?;
    if let Some(tt) = parser.peek() {
        return Err(syn::Error::new(tt.span(), "unexpected token after RSX expression"));
    }
    Ok(node)
}

struct Parser {
    tokens: Vec<TokenTree>,
    pos: usize,
}

impl Parser {
    fn new(input: TokenStream) -> Self { Self { tokens: input.into_iter().collect(), pos: 0 } }

    fn peek(&self) -> Option<&TokenTree> { self.tokens.get(self.pos) }

    fn peek2(&self) -> Option<&TokenTree> { self.tokens.get(self.pos + 1) }

    fn next(&mut self) -> Option<TokenTree> {
        let tt = self.tokens.get(self.pos).cloned();
        if tt.is_some() {
            self.pos += 1;
        }
        tt
    }

    fn expect_punct(&mut self, ch: char) -> Result<Span, syn::Error> {
        match self.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == ch => Ok(p.span()),
            Some(tt) => Err(syn::Error::new(tt.span(), format!("expected `{ch}`, found `{tt}`"))),
            None => {
                Err(syn::Error::new(
                    Span::call_site(),
                    format!("expected `{ch}`, found end of input"),
                ))
            },
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, syn::Error> {
        match self.next() {
            Some(TokenTree::Ident(i)) => Ok(i),
            Some(tt) => Err(syn::Error::new(tt.span(), format!("expected identifier, found `{tt}`"))),
            None => Err(syn::Error::new(Span::call_site(), "expected identifier, found end of input")),
        }
    }

    fn peek_close_tag(&self) -> bool {
        matches!(self.peek(),  Some(TokenTree::Punct(p)) if p.as_char() == '<')
            && matches!(self.peek2(), Some(TokenTree::Punct(p)) if p.as_char() == '/')
    }

    fn parse_node(&mut self) -> Result<RsxNode, syn::Error> {
        match self.peek() {
            // `<` — element
            Some(TokenTree::Punct(p)) if p.as_char() == '<' => self.parse_element(),

            // `{ ... }` — expression block or spread
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let group = match self.next() {
                    Some(TokenTree::Group(g)) => g,
                    _ => unreachable!(),
                };
                Ok(RsxNode::expr(group_to_string(group.stream())))
            },

            // string literal — text node
            Some(TokenTree::Literal(_)) => {
                let lit = match self.next() {
                    Some(TokenTree::Literal(l)) => l,
                    _ => unreachable!(),
                };
                let s = lit.to_string();
                // Strip surrounding quotes if present
                let content = if s.starts_with('"') && s.ends_with('"') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                };
                Ok(RsxNode::text(content))
            },

            Some(tt) => Err(syn::Error::new(tt.span(), "expected `<`, `{`, or a string literal")),
            None => Err(syn::Error::new(Span::call_site(), "unexpected end of input")),
        }
    }

    fn parse_element(&mut self) -> Result<RsxNode, syn::Error> {
        // Consume `<`
        self.expect_punct('<')?;

        // Tag name
        let tag_ident = self.expect_ident()?;
        let tag = tag_ident.to_string();

        // Attributes
        let attrs = self.parse_attrs()?;

        // Self-closing `/>` or opening `>`
        match self.peek() {
            // `/>` — self-closing
            Some(TokenTree::Punct(p)) if p.as_char() == '/' => {
                self.next(); // consume `/`
                self.expect_punct('>')?;
                Ok(RsxNode::element(tag, attrs, vec![]))
            },

            // `>` — open tag, parse children then close tag
            Some(TokenTree::Punct(p)) if p.as_char() == '>' => {
                self.next(); // consume `>`
                let children = self.parse_children()?;
                self.parse_close_tag(&tag)?;
                Ok(RsxNode::element(tag, attrs, children))
            },

            Some(tt) => Err(syn::Error::new(tt.span(), "expected `>` or `/>`")),
            None => Err(syn::Error::new(Span::call_site(), "unclosed element tag")),
        }
    }

    fn parse_children(&mut self) -> Result<Vec<RsxNode>, syn::Error> {
        let mut children = Vec::new();
        while !self.peek_close_tag() {
            match self.peek() {
                None => return Err(syn::Error::new(Span::call_site(), "unclosed element — expected closing tag")),
                _ => children.push(self.parse_node()?),
            }
        }
        Ok(children)
    }

    fn parse_close_tag(&mut self, open_tag: &str) -> Result<(), syn::Error> {
        self.expect_punct('<')?;
        self.expect_punct('/')?;
        let close_ident = self.expect_ident()?;
        if close_ident != open_tag {
            return Err(syn::Error::new(
                close_ident.span(),
                format!("mismatched tags: expected `</{open_tag}>`, found `</{close_ident}>`"),
            ));
        }
        self.expect_punct('>')?;
        Ok(())
    }

    fn parse_attrs(&mut self) -> Result<Vec<Attribute>, syn::Error> {
        let mut attrs = Vec::new();
        loop {
            match self.peek() {
                // `>` or `/` — end of attribute list
                Some(TokenTree::Punct(p)) if p.as_char() == '>' || p.as_char() == '/' => break,

                // `{ ...props }` — spread attribute
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                    let group = match self.next() {
                        Some(TokenTree::Group(g)) => g,
                        _ => unreachable!(),
                    };
                    let inner: Vec<TokenTree> = group.stream().into_iter().collect();

                    // Detect `...expr` inside the braces
                    let is_spread = matches!(
                        (inner.first(), inner.get(1)),
                        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == '.' && b.as_char() == '.'
                    ) && matches!(inner.get(2), Some(TokenTree::Punct(p)) if p.as_char() == '.');

                    if is_spread {
                        // Capture everything after `...`
                        let rest: TokenStream = inner.into_iter().skip(3).collect();
                        attrs.push(Attribute::spread(token_stream_to_string(rest)));
                    } else {
                        return Err(syn::Error::new(
                            group.span(),
                            "bare `{expr}` is not valid as an attribute — did you mean `{...expr}` for spread?",
                        ));
                    }
                },

                // `ident = value` — named attribute
                Some(TokenTree::Ident(_)) => {
                    attrs.push(self.parse_named_attr()?);
                },

                Some(tt) => return Err(syn::Error::new(tt.span(), "expected attribute name or `>`")),
                None => return Err(syn::Error::new(Span::call_site(), "unexpected end of input in attribute list")),
            }
        }
        Ok(attrs)
    }

    fn parse_named_attr(&mut self) -> Result<Attribute, syn::Error> {
        let key_ident = self.expect_ident()?;
        let key = key_ident.to_string();
        self.expect_punct('=')?;

        let value = match self.peek() {
            // `{expr}` — expression value
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let group = match self.next() {
                    Some(TokenTree::Group(g)) => g,
                    _ => unreachable!(),
                };
                AttributeValue::Expr(RsxExpr::new(group_to_string(group.stream())))
            },

            // literal value
            Some(TokenTree::Literal(_)) => {
                let lit = match self.next() {
                    Some(TokenTree::Literal(l)) => l,
                    _ => unreachable!(),
                };
                AttributeValue::Lit(parse_literal(lit)?)
            },

            // `true` / `false` — boolean literals written as idents in Rust tokens
            Some(TokenTree::Ident(i)) if i == "true" || i == "false" => {
                let b = match self.next() {
                    Some(TokenTree::Ident(i)) => i == "true",
                    _ => unreachable!(),
                };
                AttributeValue::Lit(Lit::Bool(b))
            },

            Some(tt) => return Err(syn::Error::new(tt.span(), "expected `{expr}` or a literal as attribute value")),
            None => return Err(syn::Error::new(key_ident.span(), "expected attribute value after `=`")),
        };

        Ok(Attribute::new(key, value))
    }
}

fn group_to_string(ts: TokenStream) -> String { token_stream_to_string(ts) }

fn token_stream_to_string(ts: TokenStream) -> String { ts.to_string() }

fn parse_literal(lit: Literal) -> Result<Lit, syn::Error> {
    let s = lit.to_string();

    // String literal — starts and ends with `"`
    if s.starts_with('"') && s.ends_with('"') {
        return Ok(Lit::Str(s[1..s.len() - 1].to_string()));
    }

    // Bool — handled upstream as Ident, but guard here anyway
    if s == "true" {
        return Ok(Lit::Bool(true));
    }
    if s == "false" {
        return Ok(Lit::Bool(false));
    }

    // Float — contains `.`
    if s.contains('.') {
        return s
            .parse::<f64>()
            .map(Lit::Float)
            .map_err(|_| syn::Error::new(lit.span(), format!("invalid float literal `{s}`")));
    }

    // Integer
    // Strip suffixes like `u16`, `i32`, `usize` before parsing
    let stripped = s.trim_end_matches(|c: char| c.is_alphabetic() || c == '_');
    stripped
        .parse::<i64>()
        .map(Lit::Int)
        .map_err(|_| syn::Error::new(lit.span(), format!("unsupported literal `{s}`")))
}
