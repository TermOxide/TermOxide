//! The macros `termoxide_hot_reload` can't write with `macro_rules!`.

use proc_macro::TokenStream;

/// `(::<lib>::__termoxide_app, "<lib>")`: the function `wasm_app!` defines in
/// the app's lib, and the lib's name. That is the package's name, which only
/// a proc macro can read, unless the lib's `[lib]` table renames it.
#[proc_macro]
pub fn app_lib(_input: TokenStream) -> TokenStream {
    let expansion = match std::env::var("CARGO_PKG_NAME") {
        Ok(package) => {
            let lib = package.replace('-', "_");
            format!("(::{lib}::__termoxide_app, {lib:?})")
        },
        Err(_) => r#"compile_error!("run_app! needs Cargo to build the app")"#.to_owned(),
    };
    expansion.parse().unwrap_or_default()
}
