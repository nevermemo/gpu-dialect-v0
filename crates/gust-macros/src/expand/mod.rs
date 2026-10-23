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
/// real file (a span from a synthesized token stream, as in unit tests), or
/// [`recorded_path`] could not produce a safe one, the origin is unknown and the
/// table is dropped, because lines without a file cannot be reported.
fn source_origin(kernel: &syn::ItemFn, lines: &[crate::slang::LineRecord]) -> TokenStream {
    let reported = kernel.sig.ident.span().file();
    if reported.is_empty() || reported.starts_with('<') {
        return quote!(::gust::SourceOrigin::UNKNOWN);
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok();
    origin_tokens(&reported, manifest_dir.as_deref(), lines)
}

/// [`source_origin`] once the span file and the manifest dir are known.
fn origin_tokens(
    reported: &str,
    manifest_dir: Option<&str>,
    lines: &[crate::slang::LineRecord],
) -> TokenStream {
    let file = recorded_path(reported, manifest_dir);
    if file.is_empty() || lines.is_empty() {
        return quote!(::gust::SourceOrigin::UNKNOWN);
    }
    let lines = lines.iter().map(|line| {
        let (slang_start, slang_end, rust_line) =
            (line.slang_start, line.slang_end, line.rust_line);
        quote!(::gust::LineSpan { slang_start: #slang_start, slang_end: #slang_end, rust_line: #rust_line })
    });
    quote!(::gust::SourceOrigin { file: #file, lines: &[#(#lines),*] })
}

/// The path stored in a descriptor, or `""` when no safe path can be derived. The
/// result is a whitelist, not a best effort: it is accepted only if it is relative
/// and every `/`-separated component is valid (see [`is_valid_record`]: non-empty,
/// not `.` or `..`, only ASCII letters and digits, `_ . + -` and non-ASCII
/// letters), so it can neither leak a build machine's layout nor break out of a
/// Slang string literal. Both paths are first normalised lexically (see
/// [`NormalizedPath`]: repeated separators collapsed, `.` dropped, `..` resolved),
/// then:
/// - a relative path is kept as its normalised components, unless it climbs out
///   of its root (starts with `..`), which stores the file name only;
/// - an absolute path whose components lie under `manifest_dir` (the expanding
///   crate's `CARGO_MANIFEST_DIR`) is stored relative to it;
/// - any other absolute path (Unix root, drive, UNC or `\\?\` verbatim), including
///   one that leaves the manifest dir by way of `..`, is stored as its file
///   name only.
///
/// A result that fails the whitelist is replaced by the file name, and by `""` if
/// that fails it too; [`origin_tokens`] turns `""` into an unknown origin.
fn recorded_path(reported: &str, manifest_dir: Option<&str>) -> String {
    let path = NormalizedPath::new(reported);
    let record = contained_record(&path, manifest_dir);
    if is_valid_record(&record) {
        return record;
    }
    let name = file_name(path.parts.iter().map(String::as_str));
    if is_valid_record(&name) {
        name
    } else {
        String::new()
    }
}

/// [`recorded_path`] before the whitelist check.
fn contained_record(path: &NormalizedPath, manifest_dir: Option<&str>) -> String {
    if path.root.is_empty() {
        return if path.parts.first().is_some_and(|part| part == "..") {
            file_name(path.parts.iter().map(String::as_str))
        } else {
            path.parts.join("/")
        };
    }
    if let Some(manifest_dir) = manifest_dir.filter(|_| path.root != OPAQUE_ROOT) {
        let dir = NormalizedPath::new(manifest_dir);
        // Windows paths compare case-insensitively; a false match on another
        // platform could only shorten a path that is already under the root.
        let windows = dir.root != "/";
        let same = |a: &str, b: &str| {
            if windows {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };
        if !dir.root.is_empty()
            && dir.root != OPAQUE_ROOT
            && same(&path.root, &dir.root)
            && path.parts.len() > dir.parts.len()
            && dir.parts.iter().zip(&path.parts).all(|(a, b)| same(a, b))
        {
            return path.parts[dir.parts.len()..].join("/");
        }
    }
    file_name(path.parts.iter().map(String::as_str))
}

/// What may be stored: a non-empty relative path whose every `/`-separated
/// component is valid. This also excludes a drive (`:`), a backslash and control
/// characters, which are not in the component alphabet.
fn is_valid_record(record: &str) -> bool {
    !record.is_empty() && record.split('/').all(is_valid_component)
}

/// A non-empty name of ASCII letters and digits, `_`, `.`, `+`, `-` and non-ASCII
/// letters; never `.` or `..` (nor any other name made of dots only, which
/// Windows does not keep as a distinct name).
fn is_valid_component(part: &str) -> bool {
    !part.is_empty()
        && !part.chars().all(|c| c == '.')
        && part.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '.' | '+' | '-')
                || (!c.is_ascii() && c.is_alphabetic())
        })
}

/// The last component that is not `..`.
fn file_name<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .filter(|part| *part != "..")
        .last()
        .unwrap_or_default()
        .to_owned()
}

/// The `root` of a `\\?\` verbatim path that is neither `\\?\X:\` nor
/// `\\?\UNC\`: GLOBALROOT, `Volume{GUID}`, device and drive-relative forms. It is
/// absolute and opaque, so it never counts as lying under a manifest dir.
const OPAQUE_ROOT: &str = "//?/";

/// A path split into a root and lexically normalised components. `root` is `""`
/// (relative), `/` (Unix), `X:/` (drive), `X:` (drive-relative), `//host/share`
/// (UNC) or [`OPAQUE_ROOT`]. Of the `\\?\` verbatim forms only `\\?\X:\...` and
/// `\\?\UNC\host\share\...` (any case of `UNC`) are recognised, as the drive and
/// UNC paths they name; every other one is opaque. Separators are `/` or `\`. In
/// `parts` repeated separators are collapsed, `.` is dropped and `..` pops its
/// parent; a `..` with no parent is dropped under a root and kept at the head of a
/// relative path.
struct NormalizedPath {
    root: String,
    parts: Vec<String>,
}

impl NormalizedPath {
    fn new(path: &str) -> Self {
        let path = path.replace('\\', "/");
        let mut opaque = false;
        let path = if let Some(rest) = path.strip_prefix("//?/") {
            let bytes = rest.as_bytes();
            if rest
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case("UNC/"))
            {
                format!("//{}", &rest[4..])
            } else if bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'/'
            {
                rest.to_owned()
            } else {
                opaque = true;
                rest.to_owned()
            }
        } else {
            path
        };
        let bytes = path.as_bytes();
        let (root, rest) = if opaque {
            (OPAQUE_ROOT.to_owned(), path.clone())
        } else if path.starts_with("//") {
            // The host and share are the root, not components that `..` can pop.
            let names: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            let split = names.len().min(2);
            (
                format!("//{}", names[..split].join("/")),
                names[split..].join("/"),
            )
        } else if path.starts_with('/') {
            ("/".to_owned(), path.clone())
        } else if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            let rooted = path[2..].starts_with('/');
            let separator = if rooted { "/" } else { "" };
            (format!("{}{separator}", &path[..2]), path[2..].to_owned())
        } else {
            (String::new(), path.clone())
        };
        let mut parts: Vec<String> = Vec::new();
        for part in rest.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.last().is_some_and(|last| last != "..") {
                        parts.pop();
                    } else if root.is_empty() {
                        parts.push("..".to_owned());
                    }
                }
                _ => parts.push(part.to_owned()),
            }
        }
        NormalizedPath { root, parts }
    }
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
    const REPO: Option<&str> = Some("/repo");

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
    fn recorded_path_collapses_separators_and_dot_segments_before_the_containment_check() {
        // Cases the manifest dir `/repo` and the reported path must agree on once
        // both are normalised: a doubled separator used to leave `/src/lib.rs`.
        for (reported, manifest_dir, expected) in [
            ("/repo//src/lib.rs", REPO, "src/lib.rs"),
            ("/repo/./src/lib.rs", REPO, "src/lib.rs"),
            ("/repo/src/../src/lib.rs", REPO, "src/lib.rs"),
            ("/repo/../repo/src/lib.rs", REPO, "src/lib.rs"),
            ("/../repo/src/lib.rs", REPO, "src/lib.rs"),
            ("/repo/src/.//lib.rs", Some("/repo//"), "src/lib.rs"),
            (r"C:\repo\src\lib.rs", Some(r"C:\repo"), "src/lib.rs"),
            (r"C:\repo\\src\.\lib.rs", Some(r"C:\repo"), "src/lib.rs"),
            (r"c:\REPO\src\lib.rs", Some(r"C:\repo"), "src/lib.rs"),
            (
                r"C:\repo\src\..\src\lib.rs",
                Some(r"C:\repo\."),
                "src/lib.rs",
            ),
            // Relative paths lose their `.` and repeated separators, and resolve `..`.
            ("./src/lib.rs", None, "src/lib.rs"),
            ("src//a/./lib.rs", None, "src/a/lib.rs"),
            ("src/a/../lib.rs", None, "src/lib.rs"),
        ] {
            assert_eq!(
                recorded_path(reported, manifest_dir),
                expected,
                "{reported}"
            );
            // The whitelist never has to replace these.
            assert!(
                is_valid_record(&contained(reported, manifest_dir)),
                "{reported}"
            );
        }
    }

    #[test]
    fn recorded_path_never_follows_dot_dot_out_of_the_manifest_dir_or_a_relative_root() {
        for (reported, manifest_dir, expected) in [
            // Leaves the manifest dir by way of `..` into a sibling tree.
            ("/repo/../home/alice/kernel.rs", REPO, "kernel.rs"),
            ("/repo/src/../../x.rs", REPO, "x.rs"),
            ("/repo/..", REPO, ""),
            (r"C:\repo\..\other\lib.rs", Some(r"C:\repo"), "lib.rs"),
            // A relative path that leaves its root.
            ("../other/src/lib.rs", REPO, "lib.rs"),
            ("../other/src/lib.rs", None, "lib.rs"),
            ("src/../../lib.rs", None, "lib.rs"),
            (r"..\..\lib.rs", None, "lib.rs"),
            ("..", None, ""),
        ] {
            assert_eq!(
                recorded_path(reported, manifest_dir),
                expected,
                "{reported}"
            );
            let stored = recorded_path(reported, manifest_dir);
            assert!(stored.is_empty() || is_valid_record(&stored), "{reported}");
        }
    }

    #[test]
    fn recorded_path_whitelist_replaces_a_relative_result_that_looks_absolute() {
        // `c:` as the first component of a relative path is a drive on Windows.
        assert_eq!(contained("./c:/x.rs", None), "c:/x.rs");
        assert!(!is_valid_record("c:/x.rs"));
        assert_eq!(recorded_path("./c:/x.rs", None), "x.rs");
    }

    #[test]
    fn recorded_path_rejects_drive_like_components_even_in_the_file_name() {
        // A colon inside a component can never be stored, so when the file name
        // has one too there is nothing safe left and the origin becomes unknown.
        for (reported, manifest_dir) in [
            ("/repo/C:..", REPO),
            ("/repo/C:D:x.rs", REPO),
            ("/repo/C:x.rs", REPO),
            ("C:D:x.rs", None),
            ("a/C:x.rs", None),
            (r"\\?\C:", None),
            (r"\\?\C:lib.rs", None),
            ("C:", None),
        ] {
            assert_eq!(recorded_path(reported, manifest_dir), "", "{reported}");
        }
    }

    #[test]
    fn recorded_path_treats_unrecognised_verbatim_forms_as_opaque_roots() {
        // Only `\\?\X:\` and `\\?\UNC\` name a drive or UNC path. GLOBALROOT,
        // Volume{GUID} and device forms used to lose the prefix and be kept as a
        // relative path, leaking `Users/someone`.
        for (reported, manifest_dir, expected) in [
            (
                r"\\?\GLOBALROOT\Device\HarddiskVolume1\Users\someone\kernel.rs",
                None,
                "kernel.rs",
            ),
            (
                r"\\?\GLOBALROOT\Device\HarddiskVolume1\Users\someone\kernel.rs",
                Some(r"\\?\GLOBALROOT\Device\HarddiskVolume1\Users\someone"),
                "kernel.rs",
            ),
            (
                r"\\?\Volume{12345678-1234-1234-1234-123456789abc}\src\lib.rs",
                Some(r"C:\build_gust\crates\demo"),
                "lib.rs",
            ),
            (r"\\?\pipe\kernel.rs", UNIX_ROOT, "kernel.rs"),
            (r"//?/GLOBALROOT/x/kernel.rs", None, "kernel.rs"),
            // The recognised forms still resolve against the manifest dir.
            (
                r"\\?\unc\build\share\crates\demo\src\lib.rs",
                Some(r"\\build\share\crates\demo"),
                "src/lib.rs",
            ),
            (
                r"\\?\C:\build_gust\crates\demo\src\lib.rs",
                DRIVE_ROOT,
                "src/lib.rs",
            ),
        ] {
            assert_eq!(
                recorded_path(reported, manifest_dir),
                expected,
                "{reported}"
            );
        }
    }

    #[test]
    fn recorded_path_has_no_origin_for_empty_and_dot_only_inputs() {
        for reported in [
            "", ".", "..", "./", "../..", "/", r"\", "//", "./.", "src/..",
        ] {
            for manifest_dir in [None, REPO, DRIVE_ROOT] {
                assert_eq!(recorded_path(reported, manifest_dir), "", "{reported:?}");
            }
        }
    }

    #[test]
    fn recorded_path_keeps_unicode_letters_and_rejects_everything_else() {
        for (reported, expected) in [
            ("src/ünï/kernel_é.rs", "src/ünï/kernel_é.rs"),
            ("日本語/ファイル.rs", "日本語/ファイル.rs"),
            ("src/a.b/lib-1+2.rs", "src/a.b/lib-1+2.rs"),
            // A component outside the alphabet falls back to the file name.
            ("src/my kernels/lib.rs", "lib.rs"),
            ("src/.../lib.rs", "lib.rs"),
            ("src/a\u{ff1a}b/lib.rs", "lib.rs"),
            ("src/a\u{202e}b/lib.rs", "lib.rs"),
            ("src/a\nb/lib.rs", "lib.rs"),
            ("src/a\"b/lib.rs", "lib.rs"),
            ("src/a?b/lib.rs", "lib.rs"),
            // ... and when the file name is outside it too there is no origin.
            ("src/my kernel.rs", ""),
            ("src/kernel\u{0}.rs", ""),
            ("src/k\u{ff1a}.rs", ""),
            ("src/\u{1f600}.rs", ""),
            ("/opt/other/\"quoted\".rs", ""),
        ] {
            assert_eq!(recorded_path(reported, None), expected, "{reported:?}");
        }
    }

    #[test]
    fn recorded_path_handles_genuinely_mixed_separators() {
        for (reported, manifest_dir, expected) in [
            (r"src\a/b\lib.rs", None, "src/a/b/lib.rs"),
            (r"C:/repo\src/lib.rs", Some(r"C:\repo"), "src/lib.rs"),
            (r"/repo\src\..\lib.rs", REPO, "lib.rs"),
            (r"\repo/src\lib.rs", REPO, "src/lib.rs"),
            (
                r"\\build/share\crates/demo\src/lib.rs",
                Some(r"\\build\share\crates\demo"),
                "src/lib.rs",
            ),
            (r"//?/C:\repo/src\lib.rs", Some(r"C:\repo"), "src/lib.rs"),
            (r"src\/\a//b\\lib.rs", None, "src/a/b/lib.rs"),
        ] {
            assert_eq!(
                recorded_path(reported, manifest_dir),
                expected,
                "{reported}"
            );
        }
    }

    #[test]
    fn recorded_path_is_always_empty_or_a_safe_relative_path() {
        for reported in [
            "/a/b.rs",
            r"C:\a\b.rs",
            "C:b.rs",
            r"\\h\s\b.rs",
            r"\\?\C:\a\b.rs",
            r"\\?\UNC\h\s\b.rs",
            "relative/b.rs",
            "../a/b.rs",
            r"a\..\..\b.rs",
            "/a/../../b.rs",
            "/a//b/./../c.rs",
            r"C:\a\..\..\b.rs",
            "C:..\\b.rs",
            r"\\h\s\..\..\b.rs",
            "/repo/C:..",
            "/repo/C:D:x.rs",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\Users\someone\kernel.rs",
            r"\\?\Volume{12345678-1234-1234-1234-123456789abc}\src\lib.rs",
            r"\\.\pipe\kernel.rs",
            "",
            ".",
            "..",
            "src/ünï/kernel_é.rs",
            "src/a b/c\td.rs",
            "src/a\u{0}b.rs",
            "a/\u{ff1a}/b.rs",
        ] {
            for manifest_dir in [
                None,
                UNIX_ROOT,
                DRIVE_ROOT,
                REPO,
                Some("/a"),
                Some(r"C:\a"),
                Some(r"\\?\GLOBALROOT\Device\HarddiskVolume1"),
                Some(""),
            ] {
                let stored = recorded_path(reported, manifest_dir);
                let context = format!("{reported:?} in {manifest_dir:?} -> {stored:?}");
                assert!(!stored.starts_with('/'), "{context}");
                assert!(!stored.contains('\\'), "{context}");
                assert!(!stored.contains(':'), "{context}");
                assert!(!stored.contains('?'), "{context}");
                assert!(!stored.split('/').any(|part| part == ".."), "{context}");
                assert!(
                    stored.chars().all(|c| c.is_ascii_alphanumeric()
                        || "_.+-/".contains(c)
                        || (!c.is_ascii() && c.is_alphabetic())),
                    "{context}"
                );
                assert!(stored.is_empty() || is_valid_record(&stored), "{context}");
            }
        }
    }

    #[test]
    fn an_empty_recorded_path_drops_the_whole_line_table() {
        use crate::slang::LineRecord;
        let lines = [LineRecord {
            slang_start: 3,
            slang_end: 4,
            rust_line: 12,
        }];
        let unknown = "SourceOrigin :: UNKNOWN";
        // No safe file: no origin, never `LineSpan`s with an empty file.
        for reported in [".", "..", r"\\?\C:", "/repo/C:..", "src/my kernel.rs"] {
            let output = origin_tokens(reported, REPO, &lines).to_string();
            assert!(output.ends_with(unknown), "{reported}: {output}");
            assert!(!output.contains("LineSpan"), "{reported}: {output}");
        }
        // A safe file keeps its table.
        let output = origin_tokens("/repo/src/lib.rs", REPO, &lines).to_string();
        assert!(output.contains("\"src/lib.rs\""), "{output}");
        assert!(output.contains("rust_line : 12u32"), "{output}");
        // ... and no lines is still unknown.
        let output = origin_tokens("/repo/src/lib.rs", REPO, &[]).to_string();
        assert!(output.ends_with(unknown), "{output}");
    }

    /// [`contained_record`] from the reported string.
    fn contained(reported: &str, manifest_dir: Option<&str>) -> String {
        contained_record(&NormalizedPath::new(reported), manifest_dir)
    }
}
