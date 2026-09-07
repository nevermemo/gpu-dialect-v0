//! Opt-in Slang JSON reflection for the restricted scalar/struct storage ABI.
//!
//! Slang 2026.13.1 JSON omits aggregate size, alignment and buffer stride.
//! Successful cross-checks are therefore explicitly partial, not upload safety proofs.
//! Only global structured-buffer resources are supported. Entry-point parameters
//! must be the generated `SV_DispatchThreadID` system value, not resources/uniforms.

use crate::{
    GpuPod, KernelDescriptor, ScalarKind, TypeLayout, TypeLayoutKind,
    slang::{self, Target, TemporaryDirectory},
};
use std::{collections::BTreeMap, error, fmt, fs, process::Command};

#[derive(Debug)]
pub enum Error {
    Compiler(slang::Error),
    Invalid(String),
    Mismatch(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compiler(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "invalid Slang reflection: {s}"),
            Self::Mismatch(s) => write!(f, "POD reflection mismatch: {s}"),
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Compiler(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Compiler(slang::Error::Io(e))
    }
}
fn invalid(s: impl Into<String>) -> Error {
    Error::Invalid(s.into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reflection {
    pub target: Target,
    pub resources: Vec<ReflectedResource>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectedResource {
    pub name: String,
    pub access: crate::Access,
    pub binding: u32,
    pub space: u32,
    pub element: ReflectedType,
    /// None means the compiler JSON did not report a buffer element stride.
    pub element_stride: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectedType {
    pub name: Option<String>,
    pub size: Option<u32>,
    pub kind: ReflectedTypeKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReflectedTypeKind {
    Scalar(ScalarKind),
    Struct(Vec<ReflectedField>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectedField {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub ty: ReflectedType,
}
/// A partial evidence report. `Ok` is NOT confirmation of a complete ABI match.
#[must_use = "Inspect coverage: a successful cross-check may still lack full ABI evidence"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodCrossCheck {
    pub checked_fields: usize,
    pub aggregate_size_verified: bool,
    pub element_stride_verified: bool,
    pub alignment_verified: bool,
}
impl PodCrossCheck {
    pub fn is_full_abi_match(&self) -> bool {
        self.aggregate_size_verified && self.element_stride_verified && self.alignment_verified
    }
}

/// Compile exactly this target and entry point, requesting Slang's JSON metadata.
pub fn reflect(kernel: &KernelDescriptor, target: Target) -> Result<Reflection, Error> {
    let directory = TemporaryDirectory::create()?;
    let source = directory.path().join("kernel.slang");
    let json = directory.path().join("reflection.json");
    let (target_name, extension) = match target {
        Target::Wgsl => ("wgsl", "wgsl"),
        Target::Spirv => ("spirv", "spv"),
    };
    fs::write(&source, kernel.slang_source)?;
    let mut command = Command::new("slangc");
    command.arg(source).args([
        "-target",
        target_name,
        "-entry",
        kernel.entry_point,
        "-stage",
        "compute",
    ]);
    if target == Target::Spirv {
        command.arg("-fvk-use-entrypoint-name");
    }
    let output = command
        .arg("-reflection-json")
        .arg(&json)
        .arg("-o")
        .arg(directory.path().join(format!("kernel.{extension}")))
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Compiler(slang::Error::CompilerNotFound)
            } else {
                e.into()
            }
        })?;
    if !output.status.success() {
        return Err(Error::Compiler(slang::Error::CompilationFailed {
            target,
            diagnostics: format!(
                "{}\n{}\ncompiler exit: {}\noriginating Rust kernel: {}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout),
                output.status,
                kernel.qualified_name()
            ),
        }));
    }
    Reflection::from_json(target, &fs::read_to_string(json)?)
}

impl Reflection {
    /// Parse global structured buffers and the generated dispatch-ID parameter.
    /// Entry-point resources and ordinary value/uniform parameters are unsupported.
    pub fn from_json(target: Target, source: &str) -> Result<Self, Error> {
        let root = Parser::parse(source)?;
        if let Some(entries) = root.optional("entryPoints")? {
            for entry in entries.array()? {
                if let Some(parameters) = entry.optional("parameters")? {
                    for parameter in parameters.array()? {
                        if parameter.optional("binding")?.is_some()
                            || parameter.optional("bindings")?.is_some()
                        {
                            return Err(invalid(
                                "entry-point resource/uniform bindings are unsupported",
                            ));
                        }
                        let ty = parameter.get("type")?;
                        if parameter
                            .optional("semanticName")?
                            .map(Json::string)
                            .transpose()?
                            != Some("SV_DISPATCHTHREADID")
                            || ty.get("kind")?.string()? != "vector"
                            || ty.get("elementCount")?.u32()? != 3
                            || ty.get("elementType")?.get("kind")?.string()? != "scalar"
                            || ty.get("elementType")?.get("scalarType")?.string()? != "uint32"
                        {
                            return Err(invalid(
                                "only dispatch-thread-ID entry-point parameters are supported",
                            ));
                        }
                    }
                }
            }
        }
        let mut resources = Vec::new();
        for value in root.get("parameters")?.array()? {
            let name = value.get("name")?.string()?.to_owned();
            let binding = value.get("binding")?;
            if binding.get("kind")?.string()? != "descriptorTableSlot" {
                return Err(invalid("unsupported resource binding kind"));
            }
            let index = binding.get("index")?.u32()?;
            let space = binding
                .optional("space")?
                .map(Json::u32)
                .transpose()?
                .unwrap_or(0);
            let ty = value.get("type")?;
            if ty.get("kind")?.string()? != "resource"
                || ty.get("baseShape")?.string()? != "structuredBuffer"
            {
                return Err(invalid("only structured buffers are supported"));
            }
            let access = match ty.optional("access")?.map(Json::string).transpose()? {
                None | Some("read") => crate::Access::ReadOnly,
                Some("readWrite") => crate::Access::ReadWrite,
                _ => return Err(invalid("unsupported resource access")),
            };
            if let Some(count) = binding.optional("count")? {
                if count.u32()? != 1 {
                    return Err(invalid("binding arrays are unsupported"));
                }
            }
            if resources.iter().any(|r: &ReflectedResource| {
                r.name == name || (r.binding == index && r.space == space)
            }) {
                return Err(invalid("duplicate resource name or binding"));
            }
            resources.push(ReflectedResource {
                name,
                access,
                binding: index,
                space,
                element: read_type(ty.get("resultType")?)?,
                element_stride: None,
            });
        }
        Ok(Self { target, resources })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_json_syntax_and_unicode() {
        let json =
            Parser::parse(r#"{"name":"a\uD83D\uDE80é\n","unknown":[true,false,null,-1.25e+2]}"#)
                .unwrap();
        assert_eq!(json.get("name").unwrap().string().unwrap(), "a🚀é\n");
        for source in [
            "01",
            "-",
            "1.",
            "1e+",
            "[1,]",
            r#""\uD800""#,
            r#""\uDC00""#,
            r#""\uD800\u0041""#,
            r#""\q""#,
            "\"raw\nline\"",
            "{\"x\":0,\"x\":1}",
            "true false",
        ] {
            assert!(Parser::parse(source).is_err(), "{source}");
        }
        let deep = format!("{}0{}", "[".repeat(66), "]".repeat(66));
        assert!(Parser::parse(&deep).is_err());
        for source in ["4294967296", "-1", "1.0", "1e1"] {
            assert!(Parser::parse(source).unwrap().u32().is_err());
        }
    }
}

fn read_type(value: &Json) -> Result<ReflectedType, Error> {
    let kind = match value.get("kind")?.string()? {
        "scalar" => ReflectedTypeKind::Scalar(match value.get("scalarType")?.string()? {
            "float32" => ScalarKind::F32,
            "int32" => ScalarKind::I32,
            "uint32" => ScalarKind::U32,
            _ => return Err(invalid("unsupported scalar type")),
        }),
        "struct" => {
            let mut fields = Vec::new();
            for field in value.get("fields")?.array()? {
                let name = field.get("name")?.string()?.to_owned();
                if fields.iter().any(|f: &ReflectedField| f.name == name) {
                    return Err(invalid("duplicate struct field"));
                }
                let binding = field.get("binding")?;
                if binding.get("kind")?.string()? != "uniform" {
                    return Err(invalid("unsupported field layout category"));
                }
                fields.push(ReflectedField {
                    name,
                    offset: binding.get("offset")?.u32()?,
                    size: binding.get("size")?.u32()?,
                    ty: read_type(field.get("type")?)?,
                });
            }
            if fields.is_empty() {
                return Err(invalid("empty struct layout"));
            }
            ReflectedTypeKind::Struct(fields)
        }
        _ => {
            return Err(invalid(
                "unsupported element type (only 32-bit scalars and structs)",
            ));
        }
    };
    Ok(ReflectedType {
        name: value
            .optional("name")?
            .map(Json::string)
            .transpose()?
            .map(str::to_owned),
        size: None,
        kind,
    })
}

/// Check all available compiler field metadata against `T`, retaining explicit gaps.
/// The result never certifies alignment: current Slang JSON does not expose it.
pub fn cross_check_pod<T: GpuPod>(resource: &ReflectedResource) -> Result<PodCrossCheck, Error> {
    let host = T::LAYOUT;
    if !host.is_storage_v1()
        || host.size as usize != size_of::<T>()
        || host.alignment as usize != align_of::<T>()
    {
        return Err(Error::Mismatch(
            "host layout is not a valid storage-v1 Rust layout".into(),
        ));
    }
    let mut checked_fields = 0;
    check_type(host, &resource.element, &resource.name, &mut checked_fields)?;
    if let Some(stride) = resource.element_stride {
        equal(host.stride, stride, &format!("{}.stride", resource.name))?;
    }
    Ok(PodCrossCheck {
        checked_fields,
        aggregate_size_verified: resource.element.size.is_some(),
        element_stride_verified: resource.element_stride.is_some(),
        alignment_verified: false,
    })
}
fn equal(host: u32, compiler: u32, path: &str) -> Result<(), Error> {
    if host == compiler {
        Ok(())
    } else {
        Err(Error::Mismatch(format!(
            "{path}: host {host}, compiler {compiler}"
        )))
    }
}
fn check_type(
    host: TypeLayout,
    compiler: &ReflectedType,
    path: &str,
    count: &mut usize,
) -> Result<(), Error> {
    if let Some(size) = compiler.size {
        equal(host.size, size, &format!("{path}.size"))?;
    }
    match (host.kind, &compiler.kind) {
        (TypeLayoutKind::Scalar(a), ReflectedTypeKind::Scalar(b)) if a == *b => Ok(()),
        (TypeLayoutKind::Struct(a), ReflectedTypeKind::Struct(b)) if a.len() == b.len() => {
            for (host, compiler) in a.iter().zip(b) {
                if host.name != compiler.name {
                    return Err(Error::Mismatch(format!("{path}: field name/order differs")));
                }
                let path = format!("{path}.{}", host.name);
                equal(host.offset, compiler.offset, &format!("{path}.offset"))?;
                equal(host.ty.size, compiler.size, &format!("{path}.size"))?;
                *count += 1;
                check_type(host.ty, &compiler.ty, &path, count)?;
            }
            Ok(())
        }
        _ => Err(Error::Mismatch(format!(
            "{path}: type or field count differs"
        ))),
    }
}

#[derive(Debug)]
enum Json {
    Object(BTreeMap<String, Json>),
    Array(Vec<Json>),
    String(String),
    Number(String),
    Other,
}
impl Json {
    fn optional(&self, key: &str) -> Result<Option<&Self>, Error> {
        match self {
            Self::Object(m) => Ok(m.get(key)),
            _ => Err(invalid("expected object")),
        }
    }
    fn get(&self, key: &str) -> Result<&Self, Error> {
        self.optional(key)?
            .ok_or_else(|| invalid(format!("missing {key}")))
    }
    fn array(&self) -> Result<&[Self], Error> {
        match self {
            Self::Array(a) => Ok(a),
            _ => Err(invalid("expected array")),
        }
    }
    fn string(&self) -> Result<&str, Error> {
        match self {
            Self::String(s) => Ok(s),
            _ => Err(invalid("expected string")),
        }
    }
    fn u32(&self) -> Result<u32, Error> {
        match self {
            Self::Number(s) if s.bytes().all(|b| b.is_ascii_digit()) => {
                s.parse().map_err(|_| invalid("integer exceeds u32"))
            }
            _ => Err(invalid("expected unsigned integer")),
        }
    }
}
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Parser<'a> {
    fn parse(source: &'a str) -> Result<Json, Error> {
        let mut p = Self {
            bytes: source.as_bytes(),
            pos: 0,
        };
        let value = p.value(0)?;
        p.ws();
        if p.pos != p.bytes.len() {
            return Err(invalid("trailing JSON data"));
        }
        Ok(value)
    }
    fn ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\r' | b'\n' | b'\t'))
        {
            self.pos += 1;
        }
    }
    fn take(&mut self, b: u8) -> bool {
        self.ws();
        if self.bytes.get(self.pos) == Some(&b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, b: u8) -> Result<(), Error> {
        if self.take(b) {
            Ok(())
        } else {
            Err(invalid(format!(
                "expected {} at byte {}",
                b as char, self.pos
            )))
        }
    }
    fn value(&mut self, depth: usize) -> Result<Json, Error> {
        if depth > 64 {
            return Err(invalid("JSON nesting exceeds 64"));
        }
        self.ws();
        match self.bytes.get(self.pos).copied() {
            Some(b'{') => {
                self.pos += 1;
                let mut fields = BTreeMap::new();
                if !self.take(b'}') {
                    loop {
                        self.ws();
                        let key = self.string()?;
                        self.expect(b':')?;
                        let value = self.value(depth + 1)?;
                        if fields.insert(key, value).is_some() {
                            return Err(invalid("duplicate JSON key"));
                        }
                        if self.take(b'}') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Json::Object(fields))
            }
            Some(b'[') => {
                self.pos += 1;
                let mut values = Vec::new();
                if !self.take(b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        if self.take(b']') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Json::Array(values))
            }
            Some(b'"') => self.string().map(Json::String),
            Some(b'-' | b'0'..=b'9') => self.number().map(Json::Number),
            _ => {
                for literal in [b"true".as_slice(), b"false", b"null"] {
                    if self.bytes[self.pos..].starts_with(literal) {
                        self.pos += literal.len();
                        return Ok(Json::Other);
                    }
                }
                Err(invalid(format!("invalid JSON at byte {}", self.pos)))
            }
        }
    }
    fn number(&mut self) -> Result<String, Error> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        if self.bytes.get(self.pos) == Some(&b'0') {
            self.pos += 1;
        } else {
            self.digits()?;
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            self.digits()?;
        }
        if self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.pos += 1;
            if self
                .bytes
                .get(self.pos)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.pos += 1;
            }
            self.digits()?;
        }
        Ok(String::from_utf8(self.bytes[start..self.pos].to_vec()).expect("ASCII JSON number"))
    }
    fn digits(&mut self) -> Result<(), Error> {
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        if self.pos == start {
            Err(invalid("expected digit"))
        } else {
            Ok(())
        }
    }
    fn hex(&mut self) -> Result<u16, Error> {
        let mut n = 0;
        for _ in 0..4 {
            let b = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| invalid("truncated Unicode escape"))?;
            self.pos += 1;
            n = n * 16
                + (b as char)
                    .to_digit(16)
                    .ok_or_else(|| invalid("invalid Unicode escape"))? as u16;
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<String, Error> {
        self.expect(b'"')?;
        let mut result = String::new();
        let mut start = self.pos;
        loop {
            let b = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| invalid("unterminated string"))?;
            if b == b'"' || b == b'\\' {
                result.push_str(
                    std::str::from_utf8(&self.bytes[start..self.pos])
                        .map_err(|_| invalid("invalid UTF-8"))?,
                );
                self.pos += 1;
                if b == b'"' {
                    return Ok(result);
                }
                let escape = *self
                    .bytes
                    .get(self.pos)
                    .ok_or_else(|| invalid("truncated escape"))?;
                self.pos += 1;
                result.push(match escape {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let high = self.hex()?;
                        let cp = if (0xd800..=0xdbff).contains(&high) {
                            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                                return Err(invalid("missing low surrogate"));
                            }
                            self.pos += 2;
                            let low = self.hex()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(invalid("invalid low surrogate"));
                            }
                            0x10000 + ((high as u32 - 0xd800) << 10) + (low as u32 - 0xdc00)
                        } else {
                            high as u32
                        };
                        char::from_u32(cp).ok_or_else(|| invalid("invalid Unicode scalar"))?
                    }
                    _ => return Err(invalid("invalid escape")),
                });
                start = self.pos;
            } else {
                if b < 0x20 {
                    return Err(invalid("control byte in string"));
                }
                self.pos += 1;
            }
        }
    }
}
