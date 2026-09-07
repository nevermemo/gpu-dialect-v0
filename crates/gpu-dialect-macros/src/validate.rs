use std::collections::HashSet;

use syn::visit::{self, Visit};

const BANNED_NAMES: &[&str] = &[
    "std",
    "core",
    "alloc",
    "String",
    "Vec",
    "Box",
    "Rc",
    "Arc",
    "HashMap",
    "HashSet",
    "println",
    "print",
    "eprintln",
    "dbg",
    "panic",
    "thread",
    "spawn",
    "File",
    "TcpStream",
];

/// Numeric conversions with a proven Slang lowering and GPU-verified semantics.
const CAST_TARGETS: &[&str] = &["f32", "float", "i32", "int", "u32", "uint"];

/// Rust primitives outside the four-byte scalar subset. They are not shadowed by
/// the prelude, so rustc accepts them, and `emit_type` would pass the Rust
/// spelling through to Slang unchanged.
const UNSUPPORTED_PRIMITIVES: &[&str] = &[
    "i8", "i16", "i64", "i128", "isize", "u8", "u16", "u64", "u128", "usize", "f16", "f64", "f128",
    "char", "str",
];

/// Whether an expression is an integer literal with value zero, including a
/// unary negation of one. Float zeros are excluded: floating-point division by
/// zero is defined (it yields infinity/NaN), so only integer div/rem by zero is
/// a portable-semantic hazard.
fn is_literal_zero(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Lit(literal) => matches!(
            &literal.lit,
            syn::Lit::Int(value) if value.base10_digits().bytes().all(|digit| digit == b'0')
        ),
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
            is_literal_zero(&unary.expr)
        }
        _ => false,
    }
}

pub fn validate_module(module: &syn::ItemMod) -> syn::Result<()> {
    let Some((_, items)) = &module.content else {
        return Err(syn::Error::new_spanned(
            module,
            "#[gpu] requires an inline module; use `#[gpu] mod name { ... }`",
        ));
    };

    let function_names: HashSet<String> = items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(function)
                if !function
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("kernel")) =>
            {
                Some(function.sig.ident.to_string())
            }
            _ => None,
        })
        .collect();

    for item in items {
        match item {
            syn::Item::Fn(function) => validate_function(function, &function_names)?,
            syn::Item::Struct(item) => validate_struct(item)?,
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "GPU modules may currently contain only functions and plain structs",
                ));
            }
        }
    }
    Ok(())
}

fn validate_struct(item: &syn::ItemStruct) -> syn::Result<()> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "generic GPU structs are not supported yet",
        ));
    }
    let syn::Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            item,
            "GPU structs must have named fields",
        ));
    };
    if fields.named.is_empty() {
        return Err(syn::Error::new_spanned(
            item,
            "GPU structs must contain at least one field",
        ));
    }
    if let Some(attribute) = item
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("repr"))
    {
        return Err(syn::Error::new_spanned(
            attribute,
            "GPU structs receive repr(C) automatically",
        ));
    }
    for attribute in item
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("derive"))
    {
        let derives = attribute.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        )?;
        if derives.iter().any(|path| {
            path.segments
                .last()
                .is_some_and(|segment| segment.ident == "Clone" || segment.ident == "Copy")
        }) {
            return Err(syn::Error::new_spanned(
                attribute,
                "GPU structs receive Clone and Copy automatically",
            ));
        }
    }
    let mut visitor = RestrictedVisitor::new(HashSet::new());
    visitor.visit_item_struct(item);
    visitor.finish()
}

fn validate_function(function: &syn::ItemFn, function_names: &HashSet<String>) -> syn::Result<()> {
    use crate::slang::{ParameterFlavor, parameter_flavor};

    let is_kernel = crate::expand::kernel_options(function)?.is_some();
    let invocations = function.sig.inputs.iter().filter(|argument| matches!(argument,
        syn::FnArg::Typed(argument) if parameter_flavor(&argument.ty) == ParameterFlavor::Invocation)).count();
    if is_kernel {
        if invocations != 1 {
            return Err(syn::Error::new_spanned(&function.sig, "a GPU kernel requires exactly one dispatch-thread parameter"));
        }
        if !matches!(&function.sig.output, syn::ReturnType::Default)
            && !matches!(&function.sig.output, syn::ReturnType::Type(_, ty) if matches!(ty.as_ref(), syn::Type::Tuple(tuple) if tuple.elems.is_empty())) {
            return Err(syn::Error::new_spanned(&function.sig.output, "GPU kernels must return unit"));
        }
    } else if function.sig.inputs.iter().any(|argument| matches!(argument,
        syn::FnArg::Typed(argument) if matches!(parameter_flavor(&argument.ty), ParameterFlavor::StorageRead | ParameterFlavor::StorageReadWrite | ParameterFlavor::Uniform))) {
        return Err(syn::Error::new_spanned(&function.sig, "GPU helper functions currently accept value parameters only; access resources in the kernel"));
    }
    if function.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "async is CPU-only in GPU modules",
        ));
    }
    if function.sig.unsafety.is_some() {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "unsafe is not part of the Rust-to-Slang subset",
        ));
    }
    if function.sig.abi.is_some() || !function.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "extern and generic GPU functions are not supported yet",
        ));
    }

    let mut visitor = RestrictedVisitor::new(function_names.clone());
    for argument in &function.sig.inputs {
        if let syn::FnArg::Typed(argument) = argument {
            if let syn::Pat::Ident(name) = argument.pat.as_ref() {
                match parameter_flavor(&argument.ty) {
                    ParameterFlavor::Invocation => {
                        visitor.invocations.insert(name.ident.to_string());
                    }
                    ParameterFlavor::StorageRead | ParameterFlavor::StorageReadWrite => {
                        visitor.resources.insert(name.ident.to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    visitor.visit_item_fn(function);
    visitor.finish()
}

struct RestrictedVisitor {
    function_names: HashSet<String>,
    resources: HashSet<String>,
    invocations: HashSet<String>,
    error: Option<syn::Error>,
}

impl RestrictedVisitor {
    fn new(function_names: HashSet<String>) -> Self {
        Self {
            function_names,
            resources: HashSet::new(),
            invocations: HashSet::new(),
            error: None,
        }
    }

    fn reject(&mut self, node: impl quote::ToTokens, message: &str) {
        let error = syn::Error::new_spanned(node, message);
        if let Some(existing) = &mut self.error {
            existing.combine(error);
        } else {
            self.error = Some(error);
        }
    }

    fn finish(self) -> syn::Result<()> {
        self.error.map_or(Ok(()), Err)
    }
}

impl<'ast> Visit<'ast> for RestrictedVisitor {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        if pattern.by_ref.is_some() || pattern.subpat.is_some() {
            self.reject(
                pattern,
                "reference and @ binding modes are not supported in GPU code",
            );
            return;
        }
        visit::visit_pat_ident(self, pattern);
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if !["doc", "allow", "warn", "deny", "forbid", "derive", "kernel"]
            .iter()
            .any(|name| attribute.path().is_ident(name))
        {
            self.reject(
                attribute,
                "this attribute has no proven Rust-to-Slang meaning and cannot be dropped",
            );
        }
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if name.starts_with("__gust_") || name.starts_with("__gpu_") {
            self.reject(
                ident,
                "the __gust_ and __gpu_ prefixes are reserved for generated shader symbols",
            );
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        for segment in &path.segments {
            if BANNED_NAMES.contains(&segment.ident.to_string().as_str()) {
                self.reject(
                    path,
                    "CPU-only library/type is not available inside a #[gpu] module",
                );
                return;
            }
        }
        visit::visit_path(self, path);
    }

    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if let Some(ident) = ty.path.get_ident() {
            let name = ident.to_string();
            if UNSUPPORTED_PRIMITIVES.contains(&name.as_str()) {
                self.reject(
                    ty,
                    &format!(
                        "`{name}` is not a supported GPU type; use the 32-bit scalars `f32`/`float`, `i32`/`int`, `u32`/`uint`, or `bool`"
                    ),
                );
                return;
            }
        }
        visit::visit_type_path(self, ty);
    }

    fn visit_expr_cast(&mut self, cast: &'ast syn::ExprCast) {
        let supported = match cast.ty.as_ref() {
            syn::Type::Path(path) => path
                .path
                .get_ident()
                .is_some_and(|ident| CAST_TARGETS.contains(&ident.to_string().as_str())),
            _ => false,
        };
        if !supported {
            self.reject(
                &cast.ty,
                "casts are supported only to the 32-bit scalar types `f32`/`float`, `i32`/`int`, and `u32`/`uint`",
            );
            return;
        }
        visit::visit_expr_cast(self, cast);
    }

    fn visit_expr_const(&mut self, expression: &'ast syn::ExprConst) {
        self.reject(
            expression,
            "const blocks are not in the Rust-to-Slang subset; write the value directly",
        );
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            let contains_banned_name = path
                .path
                .segments
                .iter()
                .any(|segment| BANNED_NAMES.contains(&segment.ident.to_string().as_str()));
            if contains_banned_name {
                self.reject(
                    path,
                    "CPU-only library/type is not available inside a #[gpu] module",
                );
                return;
            }
        }
        let allowed = match call.func.as_ref() {
            syn::Expr::Path(path) => path
                .path
                .get_ident()
                .is_some_and(|ident| self.function_names.contains(&ident.to_string())),
            _ => false,
        };
        if !allowed {
            self.reject(
                call,
                "only calls to helper functions declared in the same GPU module are allowed",
            );
            return;
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_binary(&mut self, binary: &'ast syn::ExprBinary) {
        let is_div_or_rem = matches!(binary.op, syn::BinOp::Div(_) | syn::BinOp::Rem(_));
        if is_div_or_rem && is_literal_zero(&binary.right) {
            self.reject(
                binary,
                "integer division or modulo by a literal zero has no portable GPU result; guard the divisor before dividing",
            );
            return;
        }
        visit::visit_expr_binary(self, binary);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if !call.args.is_empty() || call.turbofish.is_some() {
            self.reject(
                call,
                "GPU built-ins do not accept arguments or type arguments",
            );
            return;
        }
        let method = call.method.to_string();
        if !matches!(method.as_str(), "global_id" | "len") {
            self.reject(
                call,
                "only the compatibility built-ins `.global_id()` and `.len()` are supported",
            );
            return;
        }
        let receiver = match call.receiver.as_ref() {
            syn::Expr::Path(path) => path.path.get_ident().map(ToString::to_string),
            _ => None,
        };
        let valid = receiver.as_ref().is_some_and(|name| {
            if method == "len" {
                self.resources.contains(name)
            } else {
                self.invocations.contains(name)
            }
        });
        if !valid {
            self.reject(
                call,
                if method == "len" {
                    "`.len()` requires a direct storage parameter receiver"
                } else {
                    "`.global_id()` requires a direct dispatch-thread parameter receiver"
                },
            );
            return;
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        // A bare path value must be a single identifier (a local variable or
        // parameter). Multi-segment paths are associated constants or foreign
        // items (e.g. `f32::INFINITY`, `u32::MAX`), which have no Slang lowering.
        // Callee paths are handled by `visit_expr_call`, which only descends for
        // single-identifier helpers, so this fires only for bare values.
        if path.path.get_ident().is_none() {
            self.reject(
                path,
                "associated constants and foreign paths are not in the Rust-to-Slang subset; reference local variables, parameters, or same-module helpers",
            );
        }
    }

    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        if expression.rest.is_some() {
            self.reject(
                expression,
                "struct update syntax (`..base`) is not supported; list every field explicitly",
            );
            return;
        }
        visit::visit_expr_struct(self, expression);
    }

    fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
        self.reject(
            expression,
            "the `?` operator is not in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_unsafe(&mut self, expression: &'ast syn::ExprUnsafe) {
        self.reject(
            expression,
            "unsafe blocks are not in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_macro(&mut self, expression: &'ast syn::ExprMacro) {
        self.reject(
            expression,
            "macros cannot run inside a Rust-to-Slang module",
        );
    }

    fn visit_expr_closure(&mut self, expression: &'ast syn::ExprClosure) {
        self.reject(
            expression,
            "closures are not supported in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_loop(&mut self, expression: &'ast syn::ExprLoop) {
        self.reject(
            expression,
            "loops are not supported in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        self.reject(
            expression,
            "for loops are not supported in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
        self.reject(
            expression,
            "while loops are not supported in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        self.reject(
            expression,
            "match is not supported in the Rust-to-Slang subset",
        );
    }

    fn visit_expr_await(&mut self, expression: &'ast syn::ExprAwait) {
        self.reject(
            expression,
            "await is CPU-only and cannot appear in GPU code",
        );
    }

    fn visit_expr_reference(&mut self, expression: &'ast syn::ExprReference) {
        self.reject(
            expression,
            "references are not part of the Rust-to-Slang value model",
        );
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Ok((pattern, _)) = crate::slang::local_binding(&local.pat) {
            let name = pattern.ident.to_string();
            if self.resources.contains(&name) || self.invocations.contains(&name) {
                self.reject(
                    local,
                    "GPU locals cannot shadow resource or dispatch-thread parameters",
                );
                return;
            }
        }
        if crate::slang::local_binding(&local.pat).is_err() {
            self.reject(local, "GPU let bindings require a single identifier");
            return;
        }
        if local
            .init
            .as_ref()
            .is_some_and(|initial| initial.diverge.is_some())
        {
            self.reject(
                local,
                "let-else requires control-flow lowering and is not supported yet",
            );
            return;
        }
        visit::visit_local(self, local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cpu_allocation() {
        let module: syn::ItemMod = syn::parse_quote! {
            mod bad {
                #[kernel]
                pub fn bad(ctx: Invocation) {
                    let values = Vec::new();
                }
            }
        };
        let error = validate_module(&module).expect_err("Vec must be rejected");
        assert!(error.to_string().contains("CPU-only"));
    }

    #[test]
    fn accepts_vector_add_subset() {
        let module: syn::ItemMod = syn::parse_quote! {
            mod good {
                #[kernel]
                pub fn add(ctx: Invocation, a: Storage<f32>, b: Storage<f32>, mut out: StorageMut<f32>) {
                    let i = ctx.global_id().x;
                    if i < out.len() {
                        out[i] = a[i] + b[i];
                    }
                }
            }
        };
        validate_module(&module).expect("vector add should validate");
    }

    #[test]
    fn rejects_empty_or_manually_represented_gpu_structs() {
        let empty: syn::ItemMod = syn::parse_quote! {
            mod bad {
                struct Empty {}
            }
        };
        assert!(
            validate_module(&empty)
                .expect_err("empty GPU struct must fail")
                .to_string()
                .contains("at least one field")
        );

        let represented: syn::ItemMod = syn::parse_quote! {
            mod bad {
                #[repr(C)]
                struct Params { value: f32 }
            }
        };
        assert!(
            validate_module(&represented)
                .expect_err("manual representation must fail")
                .to_string()
                .contains("receive repr(C)")
        );

        let copied: syn::ItemMod = syn::parse_quote! {
            mod bad {
                #[derive(Debug, Copy)]
                struct Params { value: f32 }
            }
        };
        assert!(
            validate_module(&copied)
                .expect_err("manual Copy derive must fail")
                .to_string()
                .contains("receive Clone and Copy")
        );
    }

    #[test]
    fn accepts_non_abi_derives_on_gpu_structs() {
        let module: syn::ItemMod = syn::parse_quote! {
            mod good {
                #[derive(Debug, PartialEq)]
                struct Params { scale: f32, bias: f32 }
            }
        };
        validate_module(&module).expect("non-ABI derives should remain available");
    }
}
