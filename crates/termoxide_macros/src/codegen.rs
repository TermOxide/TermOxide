//! Code generation from the owned RSX AST.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::ast::{Attribute, AttributeValue, Element, Lit, RsxExpr, RsxNode};

const BUILTIN_TAGS: &[&str] = &["text", "Container", "Raw"];

const KNOWN_ATTRS: &[&str] = &["area", "style", "id", "key", "class", "on_event", "label", "msg", "draw"];
const COMMON_ATTRS: &[&str] = &["area", "style", "id", "key", "class", "on_event"];
/// Generate a `TokenStream` of builder calls from an [`RsxNode`] tree.
pub fn generate(node: &RsxNode) -> TokenStream { emit_node(node) }

fn emit_node(node: &RsxNode) -> TokenStream {
    match node {
        RsxNode::Element(el) => emit_element(el),
        RsxNode::Text(s) => {
            quote! { ::termoxide_rendering::builder::text(#s).build() }
        },
        RsxNode::Expr(expr) => emit_expr(expr),
    }
}

fn emit_element(el: &Element) -> TokenStream {
    let tag = el.tag.as_str();

    if !BUILTIN_TAGS.contains(&tag) {
        if tag.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return emit_component_element(el);
        }
        let msg = format!("unknown built-in tag `{tag}` — known tags: text, Container, Raw");
        return quote! { compile_error!(#msg) };
    }

    if BUILTIN_TAGS.contains(&tag) {
        for attr in &el.attrs {
            if let AttributeValue::Spread(_) = &attr.value {
                let msg = "spread attributes (`{...props}`) are not yet supported";
                return quote! { compile_error!(#msg) };
            }
            if !attr.key.is_empty() && !KNOWN_ATTRS.contains(&attr.key.as_str()) {
                let msg = format!(
                    "unknown attribute `{}` — known attributes: {}",
                    attr.key,
                    KNOWN_ATTRS.join(", ")
                );
                return quote! { compile_error!(#msg) };
            }
        }
    }

    match tag {
        "text" => emit_text_element(el),
        "Container" => emit_container_element(el),
        "Raw" => emit_raw_element(el),
        _ => unreachable!("already checked above"),
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  text(...)
// ─────────────────────────────────────────────────────────────────────────── //

fn emit_text_element(el: &Element) -> TokenStream {
    let content: TokenStream = match el.children.as_slice() {
        [] => quote! { "" },
        [RsxNode::Text(s)] => quote! { #s },
        [RsxNode::Expr(e)] => emit_expr(e),
        _ => {
            return quote! {
                compile_error!("`<text>` must have exactly one child (a string or `{expr}`)")
            };
        },
    };

    let attr_calls = emit_attr_calls(&el.attrs);

    quote! {
        ::termoxide_rendering::builder::text(#content)
            #(#attr_calls)*
            .build()
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  el(Container)
// ─────────────────────────────────────────────────────────────────────────── //

fn emit_container_element(el: &crate::ast::Element) -> TokenStream {
    let attr_calls = emit_attr_calls(&el.attrs);

    if el.children.is_empty() {
        quote! {
            ::termoxide_rendering::builder::el(::termoxide_rendering::builder::Container)
                #(#attr_calls)*
                .build()
        }
    } else {
        let children: Vec<TokenStream> = el.children.iter().map(emit_node).collect();
        quote! {
            ::termoxide_rendering::builder::el(::termoxide_rendering::builder::Container)
                #(#attr_calls)*
                .children([#(#children),*])
                .build()
        }
    }
}

fn emit_component_element(el: &Element) -> TokenStream {
    if !el.children.is_empty() {
        return quote! {
            compile_error!("components do not support children yet")
        };
    }

    let component = format_ident!("{}", el.tag);
    let props = format_ident!("{}Props", el.tag);
    let mut prop_fields = Vec::new();
    let mut common_attrs = Vec::new();

    for attr in &el.attrs {
        if matches!(attr.value, AttributeValue::Spread(_)) {
            return quote! {
                compile_error!("spread attributes are not yet supported")
            };
        }

        if COMMON_ATTRS.contains(&attr.key.as_str()) {
            common_attrs.push(emit_single_attr_call(attr));
        } else {
            let field = format_ident!("{}", attr.key);
            let value = emit_attr_value(&attr.value);
            prop_fields.push(quote! { #field: (#value).into() });
        }
    }

    quote! {
        <#component as ::termoxide_components::Component>::render(
            #props {
                #(#prop_fields),*
            }
        )
        #(#common_attrs)*
        .build()
    }
}

fn emit_raw_element(el: &Element) -> TokenStream {
    let Some(draw) = find_attr(el, "draw") else {
        return quote! {
            compile_error!("`<Raw>` requires a `draw={{...}}` attribute")
        };
    };

    let attr_calls: Vec<TokenStream> = el.attrs.iter().filter(|a| a.key != "draw").map(emit_single_attr_call).collect();

    quote! {
        ::termoxide_rendering::builder::raw(#draw)
            #(#attr_calls)*
            .build()
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Attribute → method call chains
// ─────────────────────────────────────────────────────────────────────────── //

fn emit_attr_calls(attrs: &[Attribute]) -> Vec<TokenStream> { attrs.iter().map(emit_single_attr_call).collect() }

fn emit_single_attr_call(attr: &Attribute) -> TokenStream {
    let method = format_ident!("{}", &attr.key);
    let value = emit_attr_value(&attr.value);
    quote! { .#method(#value) }
}

fn emit_attr_value(value: &AttributeValue) -> TokenStream {
    match value {
        AttributeValue::Expr(expr) => emit_expr(expr),
        AttributeValue::Lit(lit) => emit_lit(lit),
        AttributeValue::Spread(_) => {
            let msg = "spread attributes are not yet supported";
            quote! { compile_error!(#msg) }
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Expr and Lit → TokenStream
// ─────────────────────────────────────────────────────────────────────────── //

fn emit_expr(expr: &RsxExpr) -> TokenStream {
    expr.as_str().parse::<TokenStream>().unwrap_or_else(|_| {
        let msg = format!("failed to re-parse expression: `{}`", expr.as_str());
        quote! { compile_error!(#msg) }
    })
}

fn emit_lit(lit: &Lit) -> TokenStream {
    match lit {
        Lit::Str(s) => quote! { #s },
        Lit::Int(i) => {
            let i = *i;
            quote! { #i }
        },
        Lit::Float(f) => {
            let f = *f;
            quote! { #f }
        },
        Lit::Bool(b) => {
            let b = *b;
            quote! { #b }
        },
    }
}

fn find_attr(el: &Element, key: &str) -> Option<TokenStream> {
    el.attrs.iter().find(|a| a.key == key).map(|a| emit_attr_value(&a.value))
}
