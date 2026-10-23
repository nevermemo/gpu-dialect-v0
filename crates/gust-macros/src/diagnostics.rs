//! Stable diagnostic codes for every rejection `#[gpu]` reports.
//!
//! `syn::Error` carries only a message, so the code is part of the message text:
//! `[GUST-E0401] <message>`. rustc shows it as `error: [GUST-E0401] <message>`, which
//! makes the code searchable in `docs/DIAGNOSTICS.md`. Codes are a compatibility
//! surface: they never change meaning and are never reused, while the message text may be
//! reworded freely. Renumbering requires a row in `docs/DECISIONS.md`.
//!
//! Families (the hundreds digit):
//!
//! | Family | Area |
//! | --- | --- |
//! | E01xx | module and item shape: inline module, allowed items, structs, signatures, attributes |
//! | E02xx | types: unsupported primitives, casts |
//! | E03xx | expressions and calls: names, built-ins, literals, paths, bindings |
//! | E04xx | control flow and loops |
//! | E05xx | `Option` and `Result` |
//! | E06xx | atomics |
//! | E07xx | macro entry and `#[kernel]` options |
//! | E08xx | emitter-only rejections |
//! | E09xx | reserved names and shadowing |
//!
//! Every error the crate creates goes through [`error`] (or [`wrap`] for a `syn` parse
//! error); a guard test rejects any other `syn::Error` constructor in the crate.

use std::fmt::Display;

use quote::ToTokens;

/// Declares [`Code`], its stable text and [`ALL`] from one list, so a code cannot be
/// added to the enum without being listed.
macro_rules! codes {
    ($($(#[$doc:meta])* $name:ident = $number:literal => $text:literal;)*) => {
        /// A stable diagnostic code. The discriminant is the number in the text.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u16)]
        pub(crate) enum Code {
            $($(#[$doc])* $name = $number,)*
        }

        impl Code {
            /// The code as users see it, for example `"GUST-E0401"`.
            pub(crate) fn text(self) -> &'static str {
                match self {
                    $(Self::$name => $text,)*
                }
            }
        }

        /// Every code, in declaration order.
        #[cfg_attr(not(test), allow(dead_code))]
        pub(crate) const ALL: &[Code] = &[$(Code::$name,)*];
    };
}

codes! {
    // E01xx: module and item shape.
    /// `#[gpu]` on a module without inline content.
    InlineModuleRequired = 101 => "GUST-E0101";
    /// An item other than a function or a plain struct inside the module.
    UnsupportedModuleItem = 102 => "GUST-E0102";
    /// A struct with generic parameters or a `where` clause.
    GenericStruct = 103 => "GUST-E0103";
    /// A tuple or unit struct.
    StructNeedsNamedFields = 104 => "GUST-E0104";
    /// A struct without fields.
    EmptyStruct = 105 => "GUST-E0105";
    /// A hand-written `#[repr(..)]` on a GPU struct.
    ManualRepr = 106 => "GUST-E0106";
    /// A hand-written `Clone` or `Copy` derive on a GPU struct.
    ManualCloneCopy = 107 => "GUST-E0107";
    /// A `#[derive(..)]` argument list that does not parse.
    MalformedDerive = 108 => "GUST-E0108";
    /// A kernel without exactly one dispatch-thread parameter.
    KernelDispatchParameter = 109 => "GUST-E0109";
    /// A kernel with a return type other than unit.
    KernelReturnsUnit = 110 => "GUST-E0110";
    /// A helper function taking a buffer or uniform parameter.
    HelperResourceParameter = 111 => "GUST-E0111";
    /// An `async` function.
    AsyncFunction = 112 => "GUST-E0112";
    /// An `unsafe` function.
    UnsafeFunction = 113 => "GUST-E0113";
    /// An `extern` or generic function.
    ExternOrGenericFunction = 114 => "GUST-E0114";
    /// An attribute with no proven Rust-to-Slang meaning.
    UnsupportedAttribute = 115 => "GUST-E0115";

    // E02xx: types.
    /// A primitive type outside the 32-bit scalar subset.
    UnsupportedPrimitive = 201 => "GUST-E0201";
    /// A cast to a type other than `f32`, `i32` or `u32` (or their aliases).
    UnsupportedCastTarget = 202 => "GUST-E0202";

    // E03xx: expressions and calls.
    /// A CPU-only library or type name.
    CpuOnlyName = 301 => "GUST-E0301";
    /// A call to anything but a helper declared in the same module.
    UnknownCall = 302 => "GUST-E0302";
    /// A built-in method called with a turbofish.
    BuiltinTypeArguments = 303 => "GUST-E0303";
    /// A built-in method called with arguments.
    BuiltinArguments = 304 => "GUST-E0304";
    /// A method that is not a supported built-in.
    UnsupportedMethod = 305 => "GUST-E0305";
    /// `.len()` on something other than a storage parameter.
    LenReceiver = 306 => "GUST-E0306";
    /// `.global_id()` on something other than the dispatch-thread parameter.
    GlobalIdReceiver = 307 => "GUST-E0307";
    /// An associated constant or other multi-segment path used as a value.
    ForeignPath = 308 => "GUST-E0308";
    /// A `const { .. }` block.
    ConstBlock = 309 => "GUST-E0309";
    /// Struct update syntax (`..base`).
    StructUpdateSyntax = 310 => "GUST-E0310";
    /// The `?` operator.
    TryOperator = 311 => "GUST-E0311";
    /// An `unsafe { .. }` block.
    UnsafeBlock = 312 => "GUST-E0312";
    /// A macro invocation inside the module.
    MacroInModule = 313 => "GUST-E0313";
    /// A closure.
    Closure = 314 => "GUST-E0314";
    /// `.await`.
    Await = 315 => "GUST-E0315";
    /// A reference (`&x`, `&mut x`) outside the atomic call form.
    Reference = 316 => "GUST-E0316";
    /// Integer division or remainder by a literal zero.
    DivisionByLiteralZero = 317 => "GUST-E0317";
    /// A `ref`, `ref mut` or `@` binding.
    BindingMode = 318 => "GUST-E0318";
    /// A `let` pattern that is not a single identifier.
    LetNeedsIdentifier = 319 => "GUST-E0319";
    /// `let .. else`.
    LetElse = 320 => "GUST-E0320";

    // E04xx: control flow and loops.
    /// A `loop` expression.
    LoopNotSupported = 401 => "GUST-E0401";
    /// A `while` loop.
    WhileNotSupported = 402 => "GUST-E0402";
    /// A loop label or a labelled `break` or `continue`.
    LoopLabel = 403 => "GUST-E0403";
    /// `break` or `continue` outside a `for` loop.
    JumpOutsideLoop = 404 => "GUST-E0404";
    /// `break` or `continue` used as a value.
    JumpAsValue = 405 => "GUST-E0405";
    /// `break` carrying a value.
    BreakWithValue = 406 => "GUST-E0406";
    /// A `mut` loop variable.
    MutableLoopVariable = 407 => "GUST-E0407";
    /// A `for` pattern that is not one identifier or `_`.
    LoopVariablePattern = 408 => "GUST-E0408";
    /// A `for` over anything but an exclusive range.
    ForIterable = 409 => "GUST-E0409";
    /// An inclusive range (`..=`).
    InclusiveRange = 410 => "GUST-E0410";
    /// A range with a missing bound.
    OpenRange = 411 => "GUST-E0411";
    /// An integer-literal loop bound without a type suffix.
    UnsuffixedLoopBound = 412 => "GUST-E0412";
    /// A range used outside the iterable of a `for` loop.
    RangeAsValue = 413 => "GUST-E0413";

    // E05xx: Option and Result.
    /// `Option` in a struct field or resource element type.
    OptionLayout = 501 => "GUST-E0501";
    /// An `Option` payload that is not a supported scalar or module struct.
    OptionPayload = 502 => "GUST-E0502";
    /// `Result` in a struct field or resource element type.
    ResultLayout = 503 => "GUST-E0503";
    /// A `Result` that is not `Result<T, T>` with a supported payload.
    ResultShape = 504 => "GUST-E0504";
    /// `Some(..)` with other than one argument.
    SomeArguments = 505 => "GUST-E0505";
    /// `Ok(..)` or `Err(..)` with other than one argument.
    OkErrArguments = 506 => "GUST-E0506";
    /// `Some`, `Ok` or `Err` used as a bare value.
    VariantAsValue = 507 => "GUST-E0507";
    /// `is_some`, `is_none`, `is_ok` or `is_err` with arguments.
    PredicateArguments = 508 => "GUST-E0508";
    /// `unwrap_or` with other than one argument.
    UnwrapOrArguments = 509 => "GUST-E0509";
    /// `unwrap` or `expect`.
    UnwrapPanics = 510 => "GUST-E0510";
    /// An `if let` pattern other than `Some(identifier)` or `Ok(identifier)`.
    IfLetPattern = 511 => "GUST-E0511";
    /// An `if let` in a position other than the whole condition.
    IfLetPosition = 512 => "GUST-E0512";
    /// A `Result` match used as a value.
    MatchAsValue = 513 => "GUST-E0513";
    /// A `Result` match statement without its terminating `;`.
    MatchNeedsSemicolon = 514 => "GUST-E0514";
    /// A match on something that is not a known `Result<T, T>`.
    MatchScrutinee = 515 => "GUST-E0515";
    /// An attribute on a match arm.
    MatchArmAttribute = 516 => "GUST-E0516";
    /// A match guard.
    MatchGuard = 517 => "GUST-E0517";
    /// A match arm whose body is not a block.
    MatchArmBlock = 518 => "GUST-E0518";
    /// A match binding that is not a plain identifier.
    MatchBinding = 519 => "GUST-E0519";
    /// A match without exactly one `Ok` arm and one `Err` arm.
    MatchArmCount = 520 => "GUST-E0520";
    /// A match pattern other than `Ok(identifier)` or `Err(identifier)`.
    MatchPattern = 521 => "GUST-E0521";

    // E06xx: atomics.
    /// An atomic operation with the wrong number of arguments.
    AtomicArgumentCount = 601 => "GUST-E0601";
    /// An atomic operation whose first argument is not `&mut buffer[index]`.
    AtomicArgumentShape = 602 => "GUST-E0602";
    /// An atomic operation on something other than a read-write buffer parameter.
    AtomicBufferParameter = 603 => "GUST-E0603";
    /// An atomic operation on an element type other than `u32` or `i32`.
    AtomicElementType = 604 => "GUST-E0604";
    /// `atomic_min` or `atomic_max` on a signed element.
    AtomicSignedMinMax = 605 => "GUST-E0605";
    /// An atomic operation used as a value.
    AtomicAsValue = 606 => "GUST-E0606";
    /// Ordinary indexing of a buffer that is also used atomically.
    AtomicDirectAccess = 607 => "GUST-E0607";

    // E09xx: reserved names and shadowing.
    /// An identifier with a reserved `__gust_` or `__gpu_` prefix.
    ReservedPrefix = 901 => "GUST-E0901";
    /// A local that shadows a resource or dispatch-thread parameter.
    ShadowedParameter = 902 => "GUST-E0902";
}

/// The error for `code` at `span`: the message prefixed with `[GUST-E....]`.
///
/// Message texts are not stable; the code is.
pub(crate) fn error(code: Code, span: impl ToTokens, message: impl Display) -> syn::Error {
    syn::Error::new_spanned(span, format!("[{}] {message}", code.text()))
}

/// A `syn` parse error given `code`, keeping every combined error and its span.
///
/// For failures `syn` reports itself (an unparsable attribute argument list, for
/// example), where there is no message of ours to attach the code to.
pub(crate) fn wrap(code: Code, parse_error: syn::Error) -> syn::Error {
    let mut wrapped: Option<syn::Error> = None;
    for part in parse_error {
        let part = syn::Error::new(part.span(), format!("[{}] {part}", code.text()));
        match &mut wrapped {
            Some(existing) => existing.combine(part),
            None => wrapped = Some(part),
        }
    }
    wrapped.expect("a syn::Error holds at least one error")
}

/// Helpers that read codes back out of errors, for tests.
#[cfg(test)]
pub(crate) mod testing {
    use super::{ALL, Code};

    /// The code at the start of `message` (`[GUST-E0401] ...`), or `None` when uncoded.
    pub(crate) fn code_of_message(message: &str) -> Option<Code> {
        let rest = message.strip_prefix('[')?;
        let (text, _) = rest.split_once(']')?;
        ALL.iter().copied().find(|code| code.text() == text)
    }

    /// The code of every error combined in `error`, in order. Panics on an uncoded one,
    /// so a rejection that skipped [`super::error`] fails the test that sees it.
    pub(crate) fn codes_of(error: &syn::Error) -> Vec<Code> {
        error
            .clone()
            .into_iter()
            .map(|part| {
                let message = part.to_string();
                code_of_message(&message)
                    .unwrap_or_else(|| panic!("diagnostic without a code: {message}"))
            })
            .collect()
    }

    /// The codes of a rejection; empty for an accepted input.
    pub(crate) fn codes_of_result<T>(result: &syn::Result<T>) -> Vec<Code> {
        result.as_ref().err().map(codes_of).unwrap_or_default()
    }

    /// Asserts the first diagnostic of `error` has `code`. Like the substring checks it
    /// replaces, this looks at the first of any combined errors; use [`assert_codes`]
    /// to pin all of them.
    pub(crate) fn assert_code(error: &syn::Error, code: Code) {
        let codes = codes_of(error);
        assert_eq!(codes.first(), Some(&code), "{error}");
    }

    /// Asserts the codes of every combined error, in order.
    pub(crate) fn assert_codes(error: &syn::Error, expected: &[Code]) {
        assert_eq!(codes_of(error), expected, "{error}");
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::Path};

    use super::testing::{assert_code, assert_codes, code_of_message, codes_of, codes_of_result};
    use super::*;

    /// Guard (i): numbers are unique, ascending and inside a known family, and the text
    /// is the number. `ALL` is generated with the enum, so no variant can be missing.
    #[test]
    fn codes_are_unique_ordered_and_match_their_text() {
        let mut seen = HashSet::new();
        let mut previous = 0;
        for &code in ALL {
            let number = code as u16;
            assert!(seen.insert(number), "duplicate discriminant {number}");
            assert!(number > previous, "{code:?} is out of order");
            previous = number;
            assert!(
                (1..=9).contains(&(number / 100)),
                "{code:?}: unknown family"
            );
            assert!(
                !number.is_multiple_of(100),
                "{code:?}: number 0 of a family"
            );
            assert_eq!(code.text(), format!("GUST-E{number:04}"), "{code:?}");
        }
        assert_eq!(seen.len(), ALL.len());
    }

    /// The `GUST-E` codes in the headings of `docs/DIAGNOSTICS.md`, outside code fences.
    fn documented_codes() -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/DIAGNOSTICS.md");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read docs/DIAGNOSTICS.md: {error}"));
        let mut in_fence = false;
        let mut codes = Vec::new();
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
            } else if !in_fence && line.starts_with('#') {
                codes.extend(heading_codes(line));
            }
        }
        codes
    }

    /// Every `GUST-E` plus digits token in a heading line.
    fn heading_codes(line: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = line;
        while let Some(start) = rest.find("GUST-E") {
            let tail = &rest[start..];
            let end = tail["GUST-E".len()..]
                .find(|c: char| !c.is_ascii_digit())
                .map_or(tail.len(), |digits| "GUST-E".len() + digits);
            found.push(tail[..end].to_owned());
            rest = &tail[end..];
        }
        found
    }

    /// Guard (ii): the doc page and `ALL` list exactly the same codes, each once.
    #[test]
    fn every_code_has_one_documentation_heading_and_no_heading_is_unknown() {
        let documented = documented_codes();
        let unique: HashSet<&String> = documented.iter().collect();
        assert_eq!(
            unique.len(),
            documented.len(),
            "a code has more than one heading: {documented:?}"
        );
        for code in ALL {
            assert!(
                unique.contains(&code.text().to_owned()),
                "{} has no heading in docs/DIAGNOSTICS.md",
                code.text()
            );
        }
        let known: HashSet<&str> = ALL.iter().map(|code| code.text()).collect();
        for heading in &documented {
            assert!(
                known.contains(heading.as_str()),
                "docs/DIAGNOSTICS.md documents {heading}, which is not a code"
            );
        }
    }

    #[test]
    fn heading_codes_reads_only_whole_code_tokens() {
        assert_eq!(heading_codes("### GUST-E0401 loop"), ["GUST-E0401"]);
        assert_eq!(heading_codes("## E04xx control flow"), Vec::<String>::new());
        assert_eq!(
            heading_codes("### GUST-E0401 and GUST-E0402"),
            ["GUST-E0401", "GUST-E0402"]
        );
        assert_eq!(heading_codes("### GUST-E"), ["GUST-E"]);
    }

    #[test]
    fn error_puts_the_code_first_and_keeps_the_message() {
        let error = error(Code::LoopNotSupported, quote::quote!(loop {}), "no loops");
        assert_eq!(error.to_string(), "[GUST-E0401] no loops");
        assert_code(&error, Code::LoopNotSupported);
        assert_codes(&error, &[Code::LoopNotSupported]);
    }

    #[test]
    fn combined_errors_report_their_codes_in_order() {
        let mut error = super::error(Code::WhileNotSupported, quote::quote!(a), "first");
        error.combine(super::error(
            Code::LoopNotSupported,
            quote::quote!(b),
            "second",
        ));
        assert_eq!(
            codes_of(&error),
            [Code::WhileNotSupported, Code::LoopNotSupported]
        );
        assert_code(&error, Code::WhileNotSupported);
    }

    #[test]
    fn an_accepted_input_yields_no_code() {
        assert!(codes_of_result(&Ok(())).is_empty());
        let rejected: syn::Result<()> = Err(super::error(Code::Closure, quote::quote!(a), "x"));
        assert_eq!(codes_of_result(&rejected), [Code::Closure]);
    }

    #[test]
    fn message_without_a_code_is_not_a_code() {
        assert_eq!(code_of_message("plain message"), None);
        assert_eq!(code_of_message("[GUST-E9999] unknown"), None);
        assert_eq!(code_of_message("[GUST-E0401 unclosed"), None);
        assert_eq!(
            code_of_message("[GUST-E0401] known"),
            Some(Code::LoopNotSupported)
        );
    }

    #[test]
    fn wrap_prefixes_every_combined_parse_error_and_keeps_order() {
        let mut parse_error = syn::Error::new(proc_macro2::Span::call_site(), "first problem");
        parse_error.combine(syn::Error::new(
            proc_macro2::Span::call_site(),
            "second problem",
        ));
        let wrapped = wrap(Code::MalformedDerive, parse_error);
        let messages: Vec<String> = wrapped.into_iter().map(|part| part.to_string()).collect();
        assert_eq!(
            messages,
            ["[GUST-E0108] first problem", "[GUST-E0108] second problem"]
        );
    }
}
