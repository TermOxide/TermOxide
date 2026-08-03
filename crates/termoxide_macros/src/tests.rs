use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    ast::{Attribute, AttributeValue, Lit, RsxNode},
    codegen::generate,
    parser::parse,
};

fn emit(node: RsxNode) -> String { generate(&node).to_string() }

#[test]
fn ast_text_element_with_expr_child() {
    let node = RsxNode::element(
        "text",
        vec![
            Attribute::expr("area", "Rect::new(0, 0, 80, 1)"),
            Attribute::expr("style", "style"),
        ],
        vec![RsxNode::expr("content")],
    );

    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.tag, "text");
    assert_eq!(el.attrs.len(), 2);
    assert_eq!(el.children.len(), 1);
    assert!(matches!(&el.children[0], RsxNode::Expr(e) if e.as_str() == "content"));
}

#[test]
fn ast_self_closing_is_empty_children() {
    let node = RsxNode::element("Container", vec![Attribute::expr("area", "rect")], vec![]);
    let RsxNode::Element(el) = &node else { panic!() };
    assert!(el.children.is_empty());
}

#[test]
fn ast_literal_attributes() {
    let node = RsxNode::element(
        "Foo",
        vec![
            Attribute::lit("id", Lit::Str("root".into())),
            Attribute::lit("count", Lit::Int(42)),
            Attribute::lit("visible", Lit::Bool(true)),
        ],
        vec![],
    );

    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.attrs[0].key, "id");
    assert_eq!(el.attrs[1].key, "count");
    assert_eq!(el.attrs[2].key, "visible");
}

#[test]
fn ast_spread_attribute() {
    let node = RsxNode::element("Foo", vec![Attribute::spread("props")], vec![]);
    let RsxNode::Element(el) = &node else { panic!() };
    assert!(matches!(
        &el.attrs[0].value,
        AttributeValue::Spread(e) if e.as_str() == "props"
    ));
}

#[test]
fn ast_nested_elements() {
    let node = RsxNode::element("Container", vec![Attribute::expr("area", "rect")], vec![
        RsxNode::element("text", vec![], vec![RsxNode::text("hello")]),
        RsxNode::element("text", vec![], vec![RsxNode::expr("dynamic")]),
    ]);

    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.children.len(), 2);
}

fn parsed(ts: TokenStream) -> RsxNode { parse(ts).expect("parse failed") }

#[test]
fn parse_text_element_with_expr_attrs_and_child() {
    let node = parsed(quote! {
        <text area={Rect::new(0, 0, 80, 1)} style={style}>{content}</text>
    });
    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.tag, "text");
    assert_eq!(el.attrs.len(), 2);
    assert_eq!(el.children.len(), 1);
    assert!(matches!(&el.children[0], RsxNode::Expr(e) if e.as_str().contains("content")));
}

#[test]
fn parse_self_closing() {
    let node = parsed(quote! { <Container area={rect} /> });
    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.tag, "Container");
    assert!(el.children.is_empty());
}

#[test]
fn parse_literal_attrs() {
    let node = parsed(quote! { <Foo id="root" count=42 visible=true /> });
    let RsxNode::Element(el) = &node else { panic!() };
    assert!(matches!(&el.attrs[0].value, AttributeValue::Lit(Lit::Str(s)) if s == "root"));
    assert!(matches!(&el.attrs[1].value, AttributeValue::Lit(Lit::Int(42))));
    assert!(matches!(&el.attrs[2].value, AttributeValue::Lit(Lit::Bool(true))));
}

#[test]
fn parse_spread_attr() {
    let node = parsed(quote! { <Foo {...props} /> });
    let RsxNode::Element(el) = &node else { panic!() };
    assert!(matches!(&el.attrs[0].value, AttributeValue::Spread(_)));
}

#[test]
fn parse_nested() {
    let node = parsed(quote! {
        <Container area={rect}>
            <text style={s}>"hello"</text>
        </Container>
    });
    let RsxNode::Element(el) = &node else { panic!() };
    assert_eq!(el.tag, "Container");
    assert_eq!(el.children.len(), 1);
    let RsxNode::Element(child) = &el.children[0] else { panic!() };
    assert_eq!(child.tag, "text");
}

#[test]
fn parse_mismatched_tags_errors() {
    assert!(parse(quote! { <Container></Foo> }).is_err());
}

#[test]
fn codegen_text_element() {
    let node = RsxNode::element(
        "text",
        vec![
            Attribute::expr("area", "Rect :: new (0 , 0 , 80 , 1)"),
            Attribute::expr("style", "style"),
        ],
        vec![RsxNode::expr("content")],
    );
    let out = emit(node);
    assert!(out.contains("text"));
    assert!(out.contains("area"));
    assert!(out.contains("style"));
    assert!(out.contains("build"));
}

#[test]
fn codegen_container_with_child() {
    let node = RsxNode::element("Container", vec![Attribute::expr("area", "rect")], vec![RsxNode::element(
        "text",
        vec![],
        vec![RsxNode::expr("label")],
    )]);
    let out = emit(node);
    assert!(out.contains("el"));
    assert!(out.contains("Container"));
    assert!(out.contains("children"));
    assert!(out.contains("build"));
}

#[test]
fn codegen_component() {
    let node = RsxNode::element(
        "Button",
        vec![Attribute::expr("label", "\"Run\""), Attribute::expr("msg", "Msg :: Run")],
        vec![],
    );
    let out = emit(node);
    assert!(out.contains("Button"));
    assert!(out.contains("ButtonProps"));
    assert!(out.contains("build"));
}

#[test]
fn codegen_raw() {
    let node = RsxNode::element(
        "Raw",
        vec![
            Attribute::expr("draw", "|buffer, area| draw(buffer, area)"),
            Attribute::expr("area", "area"),
        ],
        vec![],
    );
    let out = emit(node);
    assert!(out.contains("raw"));
    assert!(out.contains("draw"));
    assert!(out.contains("area"));
}

#[test]
fn codegen_unknown_tag_errors() {
    let out = emit(RsxNode::element("div", vec![], vec![]));
    assert!(out.contains("compile_error"));
}

#[test]
fn codegen_user_component() {
    let out = emit(RsxNode::element("MyComponent", vec![], vec![]));
    assert!(out.contains("MyComponent"));
    assert!(out.contains("MyComponentProps"));
}

#[test]
fn codegen_unknown_attr_errors() {
    let node = RsxNode::element("text", vec![Attribute::expr("unknown_attr", "val")], vec![RsxNode::text("hi")]);
    assert!(emit(node).contains("compile_error"));
}
