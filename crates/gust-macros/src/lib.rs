use proc_macro::TokenStream;

mod expand;
mod slang;
mod validate;

#[cfg(test)]
#[path = "tests/regression.rs"]
mod regression_tests;

/// Marks an inline Rust module as GUST GPU code.
///
/// The macro validates a deliberately small Rust subset, translates each
/// kernel directly to Slang, and generates host-side handles and descriptors.
#[proc_macro_attribute]
pub fn gpu(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[gpu] does not accept arguments in the current Rust-to-Slang subset",
        )
        .into_compile_error()
        .into();
    }

    let module = syn::parse_macro_input!(item as syn::ItemMod);
    expand::expand(module)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
