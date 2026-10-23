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
        let emitted = crate::slang::emit_kernel(&original_module, original_kernel, options)?;
        let source_origin = source_origin(original_kernel, &emitted.lines);
        let slang_source = emitted.source;
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
                pub const DESCRIPTOR: ::gust::KernelDescriptor =
                    <Self as ::gust::Kernel>::DESCRIPTOR;
            }

            impl ::gust::Kernel for #original_name {
                const DESCRIPTOR: ::gust::KernelDescriptor = ::gust::KernelDescriptor {
                    module: #module_name,
                    name: stringify!(#original_name),
                    entry_point: #entry_point,
                    workgroup_size: [#x, #y, #z],
                    parameters: &[#(#parameter_descriptors),*],
                    slang_source: #slang_source,
                    source: #source_origin,
                };
            }
        });
    }

    items.insert(0, syn::parse_quote! { use ::gust::prelude::*; });
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

            pub const MODULE_DESCRIPTOR: ::gust::ModuleDescriptor =
                ::gust::ModuleDescriptor {
                    name: #module_name,
                    kernels: &[#(#descriptor_paths::DESCRIPTOR),*],
                };
        }
    })
}

/// The `source:` field of a kernel descriptor: the Rust file the kernel was
/// written in plus the line table recorded by the emitter. `file` is the kernel's
/// span file passed through [`recorded_path`], so it is never absolute (the
/// descriptor is compiled into public binaries and a path would leak the build
/// machine's layout) and uses `/` only (a later `#line` directive would embed it
/// in a Slang string literal, where `\` is an escape). When the compiler gave no
/// real file (a span from a synthesized token stream, as in unit tests), the
/// origin is unknown and the table is dropped, because lines without a file
/// cannot be reported.
fn source_origin(kernel: &syn::ItemFn, lines: &[crate::slang::LineRecord]) -> TokenStream {
    let reported = kernel.sig.ident.span().file();
    if reported.is_empty() || reported.starts_with('<') || lines.is_empty() {
        return quote!(::gust::SourceOrigin::UNKNOWN);
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok();
    let file = recorded_path(&reported, manifest_dir.as_deref());
    let lines = lines.iter().map(|line| {
        let (slang_start, slang_end, rust_line) =
            (line.slang_start, line.slang_end, line.rust_line);
        quote!(::gust::LineSpan { slang_start: #slang_start, slang_end: #slang_end, rust_line: #rust_line })
    });
    quote!(::gust::SourceOrigin { file: #file, lines: &[#(#lines),*] })
}

/// The path stored in a descriptor, never absolute and always with `/`:
/// - a relative path is kept as given;
/// - an absolute path under `manifest_dir` (the expanding crate's
///   `CARGO_MANIFEST_DIR`) is stored relative to it;
/// - any other absolute path (Unix root, drive, UNC or `\\?\` verbatim) is stored
///   as its file name only.
fn recorded_path(reported: &str, manifest_dir: Option<&str>) -> String {
    let path = normalized(reported);
    if !is_absolute(&path) {
        return path;
    }
    if let Some(manifest_dir) = manifest_dir {
        let root = normalized(manifest_dir);
        let root = root.trim_end_matches('/');
        // Windows paths compare case-insensitively; a false match on another
        // platform could only shorten a path that is already under the root.
        let windows = path.as_bytes().get(1) == Some(&b':') || path.starts_with("//");
        let under_root = path.len() > root.len() + 1
            && path.as_bytes()[root.len()] == b'/'
            && path.is_char_boundary(root.len())
            && if windows {
                path[..root.len()].eq_ignore_ascii_case(root)
            } else {
                path.starts_with(root)
            };
        if !root.is_empty() && under_root {
            return path[root.len() + 1..].to_owned();
        }
    }
    let name = path.rsplit('/').next().unwrap_or_default();
    // A drive-relative `C:name.rs` has no separator, so drop its drive.
    match name.as_bytes() {
        [drive, b':', ..] if drive.is_ascii_alphabetic() => name[2..].to_owned(),
        _ => name.to_owned(),
    }
}

/// `\` to `/`, with a `\\?\` verbatim prefix removed (`\\?\UNC\host\share`
/// becomes the UNC path `//host/share`).
fn normalized(path: &str) -> String {
    let path = path.replace('\\', "/");
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = path.strip_prefix("//?/") {
        rest.to_owned()
    } else {
        path
    }
}

/// A rooted path, a UNC path, or a drive path (`C:/x`, and the drive-relative
/// `C:x`, which is not a usable relative path either). Expects `/` separators.
fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
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
                    quote!(::gust::ParameterKind::Invocation),
                    quote!(::gust::Access::NotApplicable),
                ),
                ParameterFlavor::StorageRead => (
                    quote!(::gust::ParameterKind::Storage),
                    quote!(::gust::Access::ReadOnly),
                ),
                ParameterFlavor::StorageReadWrite => (
                    quote!(::gust::ParameterKind::Storage),
                    quote!(::gust::Access::ReadWrite),
                ),
                ParameterFlavor::Uniform => (
                    quote!(::gust::ParameterKind::Uniform),
                    quote!(::gust::Access::ReadOnly),
                ),
                ParameterFlavor::Value => (
                    quote!(::gust::ParameterKind::Value),
                    quote!(::gust::Access::NotApplicable),
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
                quote!(Some(::gust::ResourceBinding { group: 0, binding: #binding }))
            } else {
                quote!(None)
            };
            let resource_layout = if matches!(
                flavor,
                ParameterFlavor::StorageRead | ParameterFlavor::StorageReadWrite
            ) {
                let element = crate::slang::wrapper_element_type(&input.ty)?;
                quote!(Some(::gust::ResourceLayout::storage_v1(
                    <#element as ::gust::GpuPod>::LAYOUT,
                )))
            } else {
                quote!(None)
            };
            Ok(quote! {
                ::gust::ParameterDescriptor {
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
            ::gust::StructFieldLayout {
                name: #field_name,
                offset: ::core::mem::offset_of!(#ident, #field_ident) as u32,
                ty: <#ty as ::gust::GpuPod>::LAYOUT,
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

        unsafe impl ::gust::GpuPod for #ident {
            const LAYOUT: ::gust::TypeLayout = ::gust::TypeLayout::structure(
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

    #[test]
    fn descriptor_origin_is_unknown_when_the_compiler_gave_no_real_file() {
        // Outside a compiler invocation spans carry no real path, so the table is
        // dropped: lines without a file cannot be reported.
        let module: syn::ItemMod = syn::parse_str(
            "mod layout {\n#[kernel]\npub fn inspect(id: SV_DispatchThreadID, values: StructuredBuffer<f32>) {\nlet i = id.x;\n}\n}",
        )
        .unwrap();
        let output = expand(module).unwrap().to_string();
        assert!(output.contains("source : :: gust :: SourceOrigin :: UNKNOWN"));
        assert!(!output.contains("LineSpan"));
    }

    const UNIX_ROOT: Option<&str> = Some("/srv/build/gust/crates/demo");
    const DRIVE_ROOT: Option<&str> = Some(r"C:\build_gust\crates\demo");

    #[test]
    fn recorded_path_keeps_relative_paths_and_normalises_separators() {
        assert_eq!(
            recorded_path("crates/demo/src/lib.rs", UNIX_ROOT),
            "crates/demo/src/lib.rs"
        );
        assert_eq!(
            recorded_path(r"crates\demo\src\lib.rs", DRIVE_ROOT),
            "crates/demo/src/lib.rs"
        );
        assert_eq!(recorded_path("src/lib.rs", None), "src/lib.rs");
        assert_eq!(recorded_path("./src/lib.rs", None), "./src/lib.rs");
    }

    #[test]
    fn recorded_path_makes_absolute_paths_under_the_manifest_dir_relative() {
        assert_eq!(
            recorded_path("/srv/build/gust/crates/demo/src/lib.rs", UNIX_ROOT),
            "src/lib.rs"
        );
        assert_eq!(
            recorded_path(r"C:\build_gust\crates\demo\src\kernels\a.rs", DRIVE_ROOT),
            "src/kernels/a.rs"
        );
        // Drive letter case and a verbatim prefix do not defeat the match, and a
        // trailing separator on the manifest dir is harmless.
        assert_eq!(
            recorded_path(
                r"\\?\c:\build_gust\crates\demo\src\lib.rs",
                Some(r"C:\build_gust\crates\demo\")
            ),
            "src/lib.rs"
        );
        assert_eq!(
            recorded_path(
                r"\\?\UNC\build\share\crates\demo\src\lib.rs",
                Some(r"\\build\share\crates\demo")
            ),
            "src/lib.rs"
        );
    }

    #[test]
    fn recorded_path_keeps_only_the_file_name_of_absolute_paths_outside_the_manifest_dir() {
        for (reported, manifest_dir) in [
            ("/opt/other/crate/src/lib.rs", UNIX_ROOT),
            // A sibling that merely shares the manifest dir as a string prefix.
            ("/srv/build/gust/crates/demo-two/src/lib.rs", UNIX_ROOT),
            (r"D:\elsewhere\src\lib.rs", DRIVE_ROOT),
            (r"\\server\share\src\lib.rs", DRIVE_ROOT),
            (r"\\?\C:\Windows\src\lib.rs", DRIVE_ROOT),
            (r"\\?\UNC\server\share\src\lib.rs", DRIVE_ROOT),
            ("/opt/other/crate/src/lib.rs", None),
            (r"C:\build_gust\crates\demo\src\lib.rs", None),
            // The manifest dir itself is not a file inside it.
            ("/srv/build/gust/crates/demo", UNIX_ROOT),
        ] {
            let stored = recorded_path(reported, manifest_dir);
            let expected = reported.rsplit(['/', '\\']).next().unwrap().to_owned();
            assert_eq!(stored, expected, "{reported}");
            assert!(
                !stored.contains('/') && !stored.contains('\\'),
                "{reported}"
            );
        }
    }

    #[test]
    fn recorded_path_is_never_absolute() {
        for reported in [
            "/a/b.rs",
            r"C:\a\b.rs",
            "C:b.rs",
            r"\\h\s\b.rs",
            r"\\?\C:\a\b.rs",
            r"\\?\UNC\h\s\b.rs",
            "relative/b.rs",
        ] {
            for manifest_dir in [None, UNIX_ROOT, DRIVE_ROOT, Some("/a"), Some(r"C:\a")] {
                let stored = recorded_path(reported, manifest_dir);
                assert!(!is_absolute(&stored), "{reported} -> {stored}");
                assert!(!stored.contains('\\'), "{reported} -> {stored}");
                assert!(!stored.contains(':'), "{reported} -> {stored}");
            }
        }
    }
}
