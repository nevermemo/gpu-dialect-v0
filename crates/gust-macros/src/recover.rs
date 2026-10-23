//! Error recovery for `#[gpu]`: the original tokens beside `compile_error!`.
//!
//! When the macro rejects its input it used to return only the error, so rustc then
//! reported every use of the module's items as "cannot find ...". Returning the
//! user's code as well keeps those names resolvable, so an editor still shows types
//! and go-to-definition, and the one real diagnostic stays the only error. The
//! `compile_error!` is always emitted first, so the build still fails and nothing is
//! accepted silently.

use proc_macro2::TokenStream;
use quote::ToTokens;

/// `error` as a `compile_error!`, followed by the user's code.
///
/// For a parsed module the code is the module with the attributes rustc does not know
/// (`#[kernel]`) removed, the prelude import that the successful expansion also adds,
/// and warning noise from the unfinished module silenced. Without a parsed module (a
/// parse failure or an item that is not a module) the raw `item` tokens follow
/// unchanged. No kernel handle stubs are invented: items the expansion would have
/// generated (kernel structs, `DESCRIPTOR`) stay missing.
pub(crate) fn beside_error(
    error: syn::Error,
    item: &TokenStream,
    module: Option<syn::ItemMod>,
) -> TokenStream {
    let mut tokens = error.into_compile_error();
    match module {
        Some(module) => tokens.extend(recovered_module(module).into_token_stream()),
        None => tokens.extend(item.clone()),
    }
    tokens
}

/// The module as the user wrote it, made safe to hand to rustc without the macro.
fn recovered_module(mut module: syn::ItemMod) -> syn::ItemMod {
    if let Some((_, items)) = module.content.as_mut() {
        strip_kernel_attributes(items);
        items.insert(0, syn::parse_quote! { use ::gust::prelude::*; });
    }
    module.attrs.push(syn::parse_quote! {
        #[allow(dead_code, unused_variables, unused_mut, unused_imports)]
    });
    module
}

/// Removes `#[kernel]` from every function reachable from `items` (the macro strips it
/// from the functions of a successful expansion; unprocessed it is an unknown attribute).
fn strip_kernel_attributes(items: &mut [syn::Item]) {
    for item in items {
        match item {
            syn::Item::Fn(function) => strip_from(&mut function.attrs),
            syn::Item::Mod(module) => {
                if let Some((_, nested)) = module.content.as_mut() {
                    strip_kernel_attributes(nested);
                }
            }
            syn::Item::Impl(item_impl) => {
                for member in &mut item_impl.items {
                    if let syn::ImplItem::Fn(function) = member {
                        strip_from(&mut function.attrs);
                    }
                }
            }
            syn::Item::Trait(item_trait) => {
                for member in &mut item_trait.items {
                    if let syn::TraitItem::Fn(function) = member {
                        strip_from(&mut function.attrs);
                    }
                }
            }
            _ => {}
        }
    }
}

fn strip_from(attributes: &mut Vec<syn::Attribute>) {
    attributes.retain(|attribute| !attribute.path().is_ident("kernel"));
}
