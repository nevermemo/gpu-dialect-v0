use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};

use crate::slang::ParameterFlavor;

#[derive(Clone, Copy, Debug)]
pub struct KernelOptions {
    pub workgroup_size: [u32; 3],
}

pub fn expand(mut module: syn::ItemMod) -> syn::Result<TokenStream> {
    crate::validate::validate_module(&module)?;
    let original_module = module.clone();
    let module_name = module.ident.to_string();
    let (_, items) = module.content.as_mut().expect("validated inline module");
    let mut generated = Vec::new();
    let mut descriptor_paths = Vec::new();

    for item in items.iter_mut() {
        let syn::Item::Struct(item) = item else {
            continue;
        };
        item.attrs.push(syn::parse_quote!(#[repr(C)]));
        item.attrs.push(syn::parse_quote!(#[derive(Clone, Copy)]));
        generated.push(gpu_pod_impl(item)?);
    }

    for item in items.iter_mut() {
        let syn::Item::Fn(function) = item else {
            continue;
        };
        let Some(options) = kernel_options(function)? else {
            continue;
        };
        let original_name = function.sig.ident.clone();
        let original_name_text = original_name.to_string();
        let entry_point = format!("gpu_{}_{}", module_name, original_name_text);
        let typecheck_name = format_ident!("__gpu_typecheck_{}", original_name);
        let visibility = function.vis.clone();
        let parameter_descriptors = parameter_descriptors(&function.sig.inputs)?;
        let original_kernel = find_function(&original_module, &original_name_text);
        let slang_source = crate::slang::emit_kernel(&original_module, original_kernel, options)?;
        let [x, y, z] = options.workgroup_size;

        function
            .attrs
            .retain(|attribute| !attribute.path().is_ident("kernel"));
        function.sig.ident = typecheck_name;
        function.vis = syn::Visibility::Inherited;

        descriptor_paths.push(original_name.clone());
        generated.push(quote! {
            #[allow(non_camel_case_types)]
            #visibility struct #original_name;

            impl #original_name {
                pub const DESCRIPTOR: ::gpu_dialect::KernelDescriptor =
                    <Self as ::gpu_dialect::Kernel>::DESCRIPTOR;
            }

            impl ::gpu_dialect::Kernel for #original_name {
                const DESCRIPTOR: ::gpu_dialect::KernelDescriptor = ::gpu_dialect::KernelDescriptor {
                    module: #module_name,
                    name: stringify!(#original_name),
                    entry_point: #entry_point,
                    workgroup_size: [#x, #y, #z],
                    parameters: &[#(#parameter_descriptors),*],
                    slang_source: #slang_source,
                };
            }
        });
    }

    items.insert(0, syn::parse_quote! { use ::gpu_dialect::prelude::*; });
    let module_ident = &module.ident;
    let module_vis = &module.vis;
    let module_attrs = &module.attrs;
    let content = &module.content.as_ref().expect("inline module").1;
    Ok(quote! {
        #(#module_attrs)*
        #[allow(dead_code)]
        #module_vis mod #module_ident {
            #(#content)*
            #(#generated)*

            pub const MODULE_DESCRIPTOR: ::gpu_dialect::ModuleDescriptor =
                ::gpu_dialect::ModuleDescriptor {
                    name: #module_name,
                    kernels: &[#(#descriptor_paths::DESCRIPTOR),*],
                };
        }
    })
}

fn find_function<'a>(module: &'a syn::ItemMod, name: &str) -> &'a syn::ItemFn {
    module
        .content
        .as_ref()
        .expect("inline module")
        .1
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == name => Some(function),
            _ => None,
        })
        .expect("kernel exists in original module")
}

pub fn kernel_options(function: &syn::ItemFn) -> syn::Result<Option<KernelOptions>> {
    if function
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("kernel"))
        .count()
        > 1
    {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "duplicate #[kernel] attribute",
        ));
    }
    let Some(attribute) = function
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("kernel"))
    else {
        return Ok(None);
    };
    if matches!(attribute.meta, syn::Meta::Path(_)) {
        return Ok(Some(KernelOptions {
            workgroup_size: [64, 1, 1],
        }));
    }

    let mut result = None;
    attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("workgroup_size") {
            if result.is_some() {
                return Err(meta.error("duplicate workgroup_size option"));
            }
            let content;
            syn::parenthesized!(content in meta.input);
            let x: syn::LitInt = content.parse()?;
            content.parse::<syn::Token![,]>()?;
            let y: syn::LitInt = content.parse()?;
            content.parse::<syn::Token![,]>()?;
            let z: syn::LitInt = content.parse()?;
            if !content.is_empty() {
                return Err(meta.error("expected exactly three workgroup dimensions"));
            }
            let dimensions = [x.base10_parse()?, y.base10_parse()?, z.base10_parse()?];
            if dimensions.contains(&0) {
                return Err(meta.error("workgroup dimensions must be nonzero"));
            }
            result = Some(dimensions);
            Ok(())
        } else {
            Err(meta.error("expected `workgroup_size(x, y, z)`"))
        }
    })?;
    Ok(Some(KernelOptions {
        workgroup_size: result.unwrap_or([64, 1, 1]),
    }))
}

fn parameter_descriptors(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> syn::Result<Vec<TokenStream>> {
    let mut next_binding = 0_u32;
    inputs
        .iter()
        .map(|input| {
            let syn::FnArg::Typed(input) = input else {
                return Err(syn::Error::new_spanned(
                    input,
                    "GPU functions cannot use self",
                ));
            };
            let syn::Pat::Ident(pattern) = input.pat.as_ref() else {
                return Err(syn::Error::new_spanned(
                    &input.pat,
                    "GPU parameters need identifier names",
                ));
            };
            let name = pattern.ident.to_string();
            let rust_type = input.ty.to_token_stream().to_string();
            let flavor = crate::slang::parameter_flavor(&input.ty);
            let (kind, access) = match flavor {
                ParameterFlavor::Invocation => (
                    quote!(::gpu_dialect::ParameterKind::Invocation),
                    quote!(::gpu_dialect::Access::NotApplicable),
                ),
                ParameterFlavor::StorageRead => (
                    quote!(::gpu_dialect::ParameterKind::Storage),
                    quote!(::gpu_dialect::Access::ReadOnly),
                ),
                ParameterFlavor::StorageReadWrite => (
                    quote!(::gpu_dialect::ParameterKind::Storage),
                    quote!(::gpu_dialect::Access::ReadWrite),
                ),
                ParameterFlavor::Uniform => (
                    quote!(::gpu_dialect::ParameterKind::Uniform),
                    quote!(::gpu_dialect::Access::ReadOnly),
                ),
                ParameterFlavor::Value => (
                    quote!(::gpu_dialect::ParameterKind::Value),
                    quote!(::gpu_dialect::Access::NotApplicable),
                ),
            };
            let is_resource = matches!(
                flavor,
                ParameterFlavor::StorageRead
                    | ParameterFlavor::StorageReadWrite
                    | ParameterFlavor::Uniform
            );
            let binding = if is_resource {
                let binding = next_binding;
                next_binding += 1;
                quote!(Some(::gpu_dialect::ResourceBinding { group: 0, binding: #binding }))
            } else {
                quote!(None)
            };
            let resource_layout = if matches!(
                flavor,
                ParameterFlavor::StorageRead | ParameterFlavor::StorageReadWrite
            ) {
                let element = crate::slang::wrapper_element_type(&input.ty)?;
                quote!(Some(::gpu_dialect::ResourceLayout::storage_v1(
                    <#element as ::gpu_dialect::GpuPod>::LAYOUT,
                )))
            } else {
                quote!(None)
            };
            Ok(quote! {
                ::gpu_dialect::ParameterDescriptor {
                    name: #name,
                    rust_type: #rust_type,
                    kind: #kind,
                    access: #access,
                    resource_layout: #resource_layout,
                    binding: #binding,
                }
            })
        })
        .collect()
}

fn gpu_pod_impl(item: &syn::ItemStruct) -> syn::Result<TokenStream> {
    let ident = &item.ident;
    let syn::Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            item,
            "GPU POD structs require named fields",
        ));
    };
    let field_layouts = fields.named.iter().map(|field| {
        let field_ident = field.ident.as_ref().expect("named field");
        let field_name = field_ident.to_string();
        let ty = &field.ty;
        quote! {
            ::gpu_dialect::StructFieldLayout {
                name: #field_name,
                offset: ::core::mem::offset_of!(#ident, #field_ident) as u32,
                ty: <#ty as ::gpu_dialect::GpuPod>::LAYOUT,
            }
        }
    });
    let field_types = fields.named.iter().map(|field| &field.ty);
    Ok(quote! {
        const _: () = {
            assert!(::core::mem::size_of::<#ident>() > 0);
            assert!(::core::mem::size_of::<#ident>() <= u32::MAX as usize);
            assert!(
                ::core::mem::size_of::<#ident>()
                    == 0 #( + ::core::mem::size_of::<#field_types>() )*
            );
        };

        unsafe impl ::gpu_dialect::GpuPod for #ident {
            const LAYOUT: ::gpu_dialect::TypeLayout = ::gpu_dialect::TypeLayout::structure(
                stringify!(#ident),
                ::core::mem::size_of::<Self>() as u32,
                ::core::mem::align_of::<Self>() as u32,
                &[#(#field_layouts),*],
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_contains_slang_not_custom_ir_or_spirv() {
        let module: syn::ItemMod = syn::parse_quote! {
            mod layout {
                #[kernel]
                pub fn inspect(id: SV_DispatchThreadID, values: StructuredBuffer<f32>) {
                    let i = id.x;
                }
            }
        };
        let output = expand(module).unwrap().to_string();
        assert!(output.contains("slang_source"));
        assert!(!output.contains("spirv_words"));
        assert!(!output.contains("fn cpu"));
    }
}
