//! RSX syntax parser and code generator for TermOxide.

use proc_macro::TokenStream;

mod ast;
mod codegen;
mod parser;
#[cfg(test)]
mod tests;

#[proc_macro]
/// Expand an RSX element tree into TermOxide builder calls.
pub fn rsx(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let node = match parser::parse(input) {
        Ok(node) => node,
        Err(e) => return e.to_compile_error().into(),
    };

    codegen::generate(&node).into()
}
