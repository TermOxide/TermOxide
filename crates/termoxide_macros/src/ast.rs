//! Owned intermediate representation shared by the RSX parser and code generator.

/// A node in the RSX tree.
#[derive(Debug, Clone, PartialEq)]
pub enum RsxNode {
    /// An element with a tag, attributes, and children.
    Element(Element),

    /// A literal text child.
    Text(String),

    /// A Rust expression captured from braces.
    Expr(RsxExpr),
}

/// A tagged RSX element.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// Tag name as written in the source.
    pub tag: String,

    /// Attributes in source order.
    pub attrs: Vec<Attribute>,

    /// Child nodes in source order; empty for self-closing elements.
    pub children: Vec<RsxNode>,
}

/// A single attribute on an element.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Attribute name, or an empty string for a spread.
    pub key: String,

    /// Attribute value.
    pub value: AttributeValue,
}

/// The value side of an [`Attribute`].
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// A literal value.
    Lit(Lit),

    /// A Rust expression captured from braces.
    Expr(RsxExpr),

    /// A spread expression captured from `{...expr}`.
    Spread(RsxExpr),
}

/// An owned literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    /// A string literal.
    Str(String),
    /// An integer literal.
    Int(i64),
    /// A floating-point literal.
    Float(f64),
    /// A boolean literal.
    Bool(bool),
}

/// An arbitrary Rust expression captured verbatim from the token stream.
#[derive(Debug, Clone, PartialEq)]
pub struct RsxExpr(pub String);

impl RsxExpr {
    /// Create an expression from its source representation.
    pub fn new(raw: impl Into<String>) -> Self { Self(raw.into()) }

    /// Return the expression source.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl RsxNode {
    /// Create an element node.
    pub fn element(tag: impl Into<String>, attrs: Vec<Attribute>, children: Vec<RsxNode>) -> Self {
        Self::Element(Element { tag: tag.into(), attrs, children })
    }

    /// Create a text node.
    pub fn text(content: impl Into<String>) -> Self { Self::Text(content.into()) }

    /// Create an expression node.
    pub fn expr(raw: impl Into<String>) -> Self { Self::Expr(RsxExpr::new(raw)) }
}

impl Attribute {
    /// Create an attribute with an explicit value.
    pub fn new(key: impl Into<String>, value: AttributeValue) -> Self { Self { key: key.into(), value } }

    #[allow(dead_code)]
    /// Create an expression-valued attribute.
    pub fn expr(key: impl Into<String>, raw: impl Into<String>) -> Self {
        Self::new(key, AttributeValue::Expr(RsxExpr::new(raw)))
    }

    #[allow(dead_code)]
    /// Create a literal-valued attribute.
    pub fn lit(key: impl Into<String>, lit: Lit) -> Self { Self::new(key, AttributeValue::Lit(lit)) }

    /// Create a spread attribute such as `{...props}`.
    pub fn spread(raw: impl Into<String>) -> Self { Self::new("", AttributeValue::Spread(RsxExpr::new(raw))) }
}
