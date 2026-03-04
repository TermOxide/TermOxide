/// Very small `style!` helper macro.
///
/// This macro is intentionally lightweight: it returns `Style::new()` when
/// called without arguments. It also supports a single-expression form
/// `style!(expr)` which should evaluate to a `Style` (useful for
/// composing proc-macro output or builder calls inline).
///
/// The goal is to provide ergonomic inline style usage in application
/// code without heavy boilerplate. For rich compile-time DSL features
/// prefer the `oxidui_macros` crate that ships with the workspace.
#[macro_export]
macro_rules! style {
    () => {{ $crate::style_macro::Style::new() }};
    ($s:expr) => {{ $s }};
}
