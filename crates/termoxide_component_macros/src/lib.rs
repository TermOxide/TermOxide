//! Derive macro for connecting component properties to a component builder.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Error, Ident, LitStr, Result, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct ComponentArgs {
    component: Ident,
    builder: syn::Path,
    constructor: syn::Path,
}

impl Parse for ComponentArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut component = None;
        let mut builder = None;
        let mut constructor = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "component" => component = Some(parse_ident_value(input)?),
                "builder" => builder = Some(input.parse()?),
                "constructor" => constructor = Some(input.parse()?),
                _ => return Err(Error::new(key.span(), "unknown `rsx` component option")),
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            component: component.ok_or_else(|| input.error("missing `component` option"))?,
            builder: builder.ok_or_else(|| input.error("missing `builder` option"))?,
            constructor: constructor.ok_or_else(|| input.error("missing `constructor` option"))?,
        })
    }
}

fn parse_ident_value(input: ParseStream<'_>) -> Result<Ident> {
    if input.peek(Ident) {
        input.parse()
    } else {
        let value: LitStr = input.parse()?;
        value.parse()
    }
}

#[proc_macro_derive(RsxComponent, attributes(rsx))]
/// Generate a `termoxide_components::Component` implementation.
///
/// The `rsx` attribute must provide `component`, `builder`, and `constructor`
/// paths, for example `#[rsx(component = Button, builder = ButtonBuilder,
/// constructor = button)]`.
pub fn derive_rsx_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> Result<proc_macro2::TokenStream> {
    let name = input.ident;
    let visibility = input.vis;
    let args = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("rsx"))
        .ok_or_else(|| Error::new(name.span(), "missing `#[rsx(...)]` component options"))?
        .parse_args::<ComponentArgs>()?;

    let fields = match input.data {
        syn::Data::Struct(data) => match data.fields {
            syn::Fields::Named(fields) => fields.named,
            _ => return Err(Error::new(name.span(), "component props must have named fields")),
        },
        _ => return Err(Error::new(name.span(), "component props must be a struct")),
    };

    let field_names = fields
        .iter()
        .map(|field| field.ident.clone())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Error::new(name.span(), "component props must have named fields"))?;
    let component = args.component;
    let builder = args.builder;
    let constructor = args.constructor;

    Ok(quote! {
        #visibility struct #component;

        impl ::termoxide_components::Component for #component {
            type Props = #name;
            type Builder = #builder;

            fn render(props: Self::Props) -> Self::Builder {
                #constructor(
                    #(props.#field_names),*
                )
            }
        }
    })
}
