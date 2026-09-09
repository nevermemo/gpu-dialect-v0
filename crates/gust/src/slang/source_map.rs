#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceMapSegment {
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) kernel: String,
    pub(crate) construct: &'static str,
    pub(crate) ordinal: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceMap {
    segments: Vec<SourceMapSegment>,
}

impl SourceMap {
    pub(crate) fn from_slang(source: &str) -> Self {
        let mut map = Self::default();
        let mut kernel = None;
        let mut ordinal = 0;
        for (index, line) in source.lines().enumerate() {
            let line_number = index + 1;
            if let Some(name) = line.trim().strip_prefix("// @rust kernel: ") {
                kernel = (!name.trim().is_empty()).then(|| name.trim().to_owned());
                ordinal = 0;
                continue;
            }
            let Some(kernel) = &kernel else {
                continue;
            };
            let Some(construct) = construct_kind(line.trim()) else {
                continue;
            };
            map.segments.push(SourceMapSegment {
                start_line: line_number,
                end_line: line_number,
                kernel: kernel.clone(),
                construct,
                ordinal,
            });
            ordinal += 1;
        }
        map
    }

    pub(crate) fn origin_at(&self, line: usize) -> Option<&SourceMapSegment> {
        self.segments
            .iter()
            .rev()
            .find(|segment| segment.start_line <= line && line <= segment.end_line)
            .or_else(|| {
                self.segments
                    .iter()
                    .rev()
                    .find(|segment| segment.start_line <= line)
            })
    }

    pub(crate) fn json(&self) -> String {
        let mut output = String::from("{\n  \"schema_version\": 1,\n  \"segments\": [");
        for (index, segment) in self.segments.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push_str(&format!(
                "\n    {{\"slang_start\":{},\"slang_end\":{},\"kernel\":\"{}\",\"construct\":\"{}\",\"ordinal\":{}}}",
                segment.start_line,
                segment.end_line,
                json_escape(&segment.kernel),
                segment.construct,
                segment.ordinal,
            ));
        }
        output.push_str("\n  ]\n}\n");
        output
    }

    #[cfg(test)]
    pub(crate) fn segments(&self) -> &[SourceMapSegment] {
        &self.segments
    }
}

fn construct_kind(line: &str) -> Option<&'static str> {
    if line.starts_with("// @gust construct: match #") {
        Some("match")
    } else if line.starts_with("var ") || line.starts_with("uint ") || line.starts_with("int ") {
        Some("local")
    } else if line.starts_with("if (") {
        Some("conditional")
    } else if line.starts_with("for (") {
        Some("loop")
    } else if line.starts_with("return ") {
        Some("return")
    } else if line.contains('=')
        && !line.contains("==")
        && !line.contains(">=")
        && !line.contains("<=")
    {
        Some("assignment")
    } else if line.ends_with(';') && !line.starts_with("//") && line != ";" {
        Some("expression")
    } else {
        None
    }
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
