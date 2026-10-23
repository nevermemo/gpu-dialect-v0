use proc_macro::TokenStream;

mod diagnostics;
mod expand;
mod recover;
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
    gpu_impl(attribute.into(), item.into()).into()
}

/// [`gpu`] on `proc_macro2` tokens, so it can be tested outside a proc-macro context.
///
/// Every failure returns the `compile_error!` followed by the user's own code (see
/// [`recover`]), so rustc and editors still resolve the module's items and the one real
/// diagnostic is not buried in follow-on "cannot find" errors. The attribute-argument
/// error is reported before a parse error, as before.
pub(crate) fn gpu_impl(
    attribute: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let parsed = syn::parse2::<syn::ItemMod>(item.clone());
    if !attribute.is_empty() {
        let error = syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[gpu] does not accept arguments in the current Rust-to-Slang subset",
        );
        return recover::beside_error(error, &item, parsed.ok());
    }
    let module = match parsed {
        Ok(module) => module,
        Err(error) => return recover::beside_error(error, &item, None),
    };
    match expand::expand(module.clone()) {
        Ok(tokens) => tokens,
        Err(error) => recover::beside_error(error, &item, Some(module)),
    }
}
