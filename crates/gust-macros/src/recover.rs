//! Error recovery for `#[gpu]`: the original tokens beside `compile_error!`.
//!
//! When the macro rejects its input it used to return only the error. Returning the
//! user's code as well keeps the module's items present, so an editor (rust-analyzer
//! expands the macro and reads what it returns) can still show types and go-to-definition
//! in code that failed validation. The `compile_error!` is always emitted first, so the
//! build still fails and nothing is accepted silently.
//!
//! Measured with rustc 1.98.0 on `examples/vector-add` with one rejection inside the
//! `#[gpu]` module (whose kernel `app.rs` also references through `DESCRIPTOR`):
//! `cargo check` reports exactly one error both without and with this recovery, and no
//! error about the missing items in either case. The recovery therefore does not change
//! the command-line error count; its benefit is for editors and is not measured here.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident};

/// `error` as a `compile_error!`, followed by the user's code.
///
/// For a parsed inline module the code is the module with the attributes rustc does not
/// know (`#[kernel]`) removed, kernel functions renamed and made private as the
/// successful expansion does, the prelude import that the expansion also adds, and
/// warning noise from the unfinished module silenced. Without a parsed module (a parse
/// failure or an item that is not a module) the raw `item` tokens follow unchanged. A
/// module without inline content (`mod m;`) gets the error alone: re-emitting it would
/// make rustc look for a file. No kernel handle stubs are invented: items the expansion
/// would have generated (kernel structs, `DESCRIPTOR`) stay missing.
pub(crate) fn beside_error(
    error: syn::Error,
    item: &TokenStream,
    module: Option<syn::ItemMod>,
) -> TokenStream {
    let mut tokens = error.into_compile_error();
    match module {
        Some(module) => {
            if let Some(module) = recovered_module(module) {
                tokens.extend(module.into_token_stream());
            }
        }
        None => tokens.extend(item.clone()),
    }
    tokens
}

/// The module as the user wrote it, made safe to hand to rustc without the macro, or
/// `None` when it has no inline content to recover.
fn recovered_module(mut module: syn::ItemMod) -> Option<syn::ItemMod> {
    let (_, items) = module.content.as_mut()?;
    for item in items.iter_mut() {
        if let syn::Item::Fn(function) = item
            && is_kernel(&function.attrs)
        {
            // The same shape the successful expansion gives a kernel (`expand/mod.rs`):
            // a private type-checking shadow, so a callable host `add` is never offered.
            function.sig.ident = format_ident!("__gpu_typecheck_{}", function.sig.ident);
            function.vis = syn::Visibility::Inherited;
        }
    }
    strip_kernel_attributes(items);
    items.insert(0, syn::parse_quote! { use ::gust::prelude::*; });
    module.attrs.push(syn::parse_quote! {
        #[allow(dead_code, unused_variables, unused_mut, unused_imports)]
    });
    Some(module)
}

fn is_kernel(attributes: &[syn::Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("kernel"))
}

/// Removes `#[kernel]` from the attribute lists of the items reachable from `items`
/// (the macro strips it from the functions of a successful expansion; unprocessed it is
/// an unknown attribute). A walk with `syn::visit_mut` would also reach functions nested
/// in bodies, but it needs a `syn` feature the workspace does not enable.
fn strip_kernel_attributes(items: &mut [syn::Item]) {
    for item in items {
        match item {
            syn::Item::Fn(item) => strip_from(&mut item.attrs),
            syn::Item::Struct(item) => strip_from(&mut item.attrs),
            syn::Item::Enum(item) => strip_from(&mut item.attrs),
            syn::Item::Const(item) => strip_from(&mut item.attrs),
            syn::Item::Static(item) => strip_from(&mut item.attrs),
            syn::Item::Type(item) => strip_from(&mut item.attrs),
            syn::Item::Mod(module) => {
                strip_from(&mut module.attrs);
                if let Some((_, nested)) = module.content.as_mut() {
                    strip_kernel_attributes(nested);
                }
            }
            syn::Item::Impl(item_impl) => {
                strip_from(&mut item_impl.attrs);
                for member in &mut item_impl.items {
                    match member {
                        syn::ImplItem::Fn(item) => strip_from(&mut item.attrs),
                        syn::ImplItem::Const(item) => strip_from(&mut item.attrs),
                        _ => {}
                    }
                }
            }
            syn::Item::Trait(item_trait) => {
                strip_from(&mut item_trait.attrs);
                for member in &mut item_trait.items {
                    match member {
                        syn::TraitItem::Fn(item) => strip_from(&mut item.attrs),
                        syn::TraitItem::Const(item) => strip_from(&mut item.attrs),
                        _ => {}
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
