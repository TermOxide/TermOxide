use proc_macro::TokenStream;

#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    // For now just return the input unchanged
    input
}

#[cfg(test)]
mod tests {
    use super::rsx;

    #[test]
    fn dummy_test() {
        let result = rsx(proc_macro::TokenStream::new());
        assert_eq!(result.to_string(), "");
    }
}
