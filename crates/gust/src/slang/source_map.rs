//! Sidecar diagnostic map derived directly from emitted Slang text (DX19).
//!
//! The map is a line-range index over the generated source, not an IR. Emission
//! is Allman style with one statement per line, so brace depth is enough to turn
//! a construct's first line into a range that ends at its closing brace.

use crate::SourceOrigin;

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
    /// In order of first line, so a later segment containing a line is always
    /// nested inside every earlier segment that also contains it.
    segments: Vec<SourceMapSegment>,
    /// One `// @rust kernel:` marker per named kernel: line number and name.
    markers: Vec<(usize, String)>,
}

/// A segment whose closing line is not known yet.
#[derive(Debug)]
struct OpenSegment {
    index: usize,
    /// Brace depth on the segment's first line, before that line's braces.
    base: usize,
    /// Whether a brace deeper than `base` has been seen since the segment began.
    entered: bool,
}

struct KernelState {
    name: String,
    /// Next ordinal for a construct segment; prologue and kernel do not use it.
    ordinal: usize,
    prologue_ordinal: usize,
    open: Vec<OpenSegment>,
}

impl SourceMap {
    pub(crate) fn from_slang(source: &str) -> Self {
        let lines: Vec<&str> = source.lines().collect();
        let mut map = Self::default();
        let mut state: Option<KernelState> = None;
        let mut depth = 0usize;
        for (index, raw) in lines.iter().enumerate() {
            let number = index + 1;
            let trimmed = raw.trim();
            if let Some(name) = trimmed.strip_prefix("// @rust kernel: ") {
                // A marker ends a kernel that never closed.
                if let Some(previous) = state.take() {
                    map.close_all(previous.open, number - 1);
                }
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                map.markers.push((number, name.to_owned()));
                let segment = map.push(number, name, "kernel", 0);
                state = Some(KernelState {
                    name: name.to_owned(),
                    ordinal: 0,
                    prologue_ordinal: 0,
                    open: vec![OpenSegment {
                        index: segment,
                        base: depth,
                        entered: false,
                    }],
                });
                continue;
            }
            let code = trimmed.split("//").next().unwrap_or("").trim();
            let depth_before = depth;
            if let Some(kernel) = state.as_mut() {
                map.classify(kernel, trimmed, code, number, depth_before);
            }
            // Braces inside a statement (array literals) do not open a block.
            let statement = code.ends_with(';');
            for character in code.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        if let Some(kernel) = state.as_mut().filter(|_| !statement) {
                            for open in &mut kernel.open {
                                // A header that starts on this line is judged once the
                                // whole line is counted: `if (f({ 1, 2 }))` stays open.
                                if depth > open.base
                                    && map.segments[open.index].start_line != number
                                {
                                    open.entered = true;
                                }
                            }
                        }
                    }
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if let Some(kernel) = state.as_mut().filter(|_| !statement) {
                for open in &mut kernel.open {
                    if depth > open.base && map.segments[open.index].start_line == number {
                        open.entered = true;
                    }
                }
            }
            let Some(kernel) = state.as_mut() else {
                continue;
            };
            let next_is_else = lines
                .get(index + 1)
                .is_some_and(|next| is_else(next.split("//").next().unwrap_or("").trim()));
            // Only a closing brace can end a segment; a line such as `else` between
            // two blocks must not close the conditional it continues. Like an `if` or
            // `for` header, an `else if` header whose condition holds an array literal
            // contains `}` without closing anything, unless the whole line ends with it.
            let else_header = is_else(code) && !code.ends_with('}');
            let closes = code.contains('}') && !else_header;
            while let Some(top) = kernel.open.last().filter(|_| closes) {
                let construct = map.segments[top.index].construct;
                let continues_with_else =
                    next_is_else && matches!(construct, "conditional" | "match");
                if top.entered && depth <= top.base && !continues_with_else {
                    map.segments[top.index].end_line = number;
                } else if !top.entered && depth < top.base {
                    // The enclosing block ended before this segment opened a block.
                    let start = map.segments[top.index].start_line;
                    map.segments[top.index].end_line = (number - 1).max(start);
                } else {
                    break;
                }
                kernel.open.pop();
            }
            if kernel.open.is_empty() {
                state = None;
            }
        }
        if let Some(kernel) = state {
            map.close_all(kernel.open, lines.len());
        }
        map
    }

    fn push(
        &mut self,
        line: usize,
        kernel: &str,
        construct: &'static str,
        ordinal: usize,
    ) -> usize {
        self.segments.push(SourceMapSegment {
            start_line: line,
            end_line: line,
            kernel: kernel.to_owned(),
            construct,
            ordinal,
        });
        self.segments.len() - 1
    }

    fn close_all(&mut self, open: Vec<OpenSegment>, end: usize) {
        for open in open {
            let segment = &mut self.segments[open.index];
            segment.end_line = end.max(segment.start_line);
        }
    }

    /// Record the construct that starts on this line, if any.
    fn classify(
        &mut self,
        kernel: &mut KernelState,
        trimmed: &str,
        code: &str,
        number: usize,
        depth: usize,
    ) {
        if let Some(marker) = trimmed.strip_prefix("// @gust construct: match #") {
            // A marker at the same depth ends an earlier marker that opened no block.
            let mut kept = Vec::new();
            for open in std::mem::take(&mut kernel.open) {
                let stale = !open.entered
                    && open.base == depth
                    && self.segments[open.index].construct == "match";
                if stale {
                    let segment = &mut self.segments[open.index];
                    segment.end_line = (number - 1).max(segment.start_line);
                } else {
                    kept.push(open);
                }
            }
            kernel.open = kept;
            // The label is the marker's own statement index, as the emitter wrote it.
            let label = marker.trim().parse().ok();
            // The emitter always writes a number; anything else is an emitter bug. Tests
            // fail on it, while a release build still maps the block by its map ordinal.
            debug_assert!(
                label.is_some(),
                "unparsable match marker number in `{trimmed}`"
            );
            self.open_block(kernel, number, "match", depth, label);
            return;
        }
        if is_prologue(code) {
            let extends = self.segments.last().is_some_and(|last| {
                last.construct == "prologue"
                    && last.kernel == kernel.name
                    && last.end_line + 1 == number
            });
            if extends {
                if let Some(last) = self.segments.last_mut() {
                    last.end_line = number;
                }
            } else {
                self.push(number, &kernel.name, "prologue", kernel.prologue_ordinal);
                kernel.prologue_ordinal += 1;
            }
            return;
        }
        // Lowered `match` scaffolding (scrutinee temporary, branch test and arm
        // bindings) belongs to the enclosing match segment; user statements inside
        // the arms still get their own nested segments.
        let in_match = kernel
            .open
            .iter()
            .any(|open| self.segments[open.index].construct == "match");
        if in_match && code.contains("__gust_match_") {
            return;
        }
        let Some(construct) = construct_kind(code) else {
            return;
        };
        // A header opens a block unless the whole construct sits on this line, which
        // then ends with its own `}`. Braces inside the condition (array literals)
        // do not make it a one-liner.
        if matches!(construct, "conditional" | "loop") && !code.ends_with('}') {
            self.open_block(kernel, number, construct, depth, None);
        } else {
            let ordinal = kernel.ordinal;
            kernel.ordinal += 1;
            self.push(number, &kernel.name, construct, ordinal);
        }
    }

    fn open_block(
        &mut self,
        kernel: &mut KernelState,
        number: usize,
        construct: &'static str,
        depth: usize,
        label: Option<usize>,
    ) {
        // A marked construct keeps the emitter's number; the per-kernel counter
        // still advances so unmarked constructs number the same as before.
        let ordinal = label.unwrap_or(kernel.ordinal);
        kernel.ordinal += 1;
        let index = self.push(number, &kernel.name, construct, ordinal);
        kernel.open.push(OpenSegment {
            index,
            base: depth,
            entered: false,
        });
    }

    /// The innermost segment containing `line`; failing that, the nearest
    /// preceding kernel marker as a `kernel` segment.
    pub(crate) fn origin_at(&self, line: usize) -> Option<SourceMapSegment> {
        if let Some(segment) = self
            .segments
            .iter()
            .rev()
            .find(|segment| segment.start_line <= line && line <= segment.end_line)
        {
            return Some(segment.clone());
        }
        self.markers
            .iter()
            .rev()
            .find(|(marker, _)| *marker <= line)
            .map(|(marker, name)| SourceMapSegment {
                start_line: *marker,
                end_line: line,
                kernel: name.clone(),
                construct: "kernel",
                ordinal: 0,
            })
    }

    /// Schema version 2: `rust_file` is the kernel's Rust source path or `null`,
    /// and a segment carries `rust_line` only when `origin` has a record for its
    /// first Slang line. The key is omitted otherwise.
    pub(crate) fn json(&self, origin: &SourceOrigin) -> String {
        let rust_file = if origin.file.is_empty() {
            "null".to_owned()
        } else {
            format!("\"{}\"", json_escape(origin.file))
        };
        let mut output = format!(
            "{{\n  \"schema_version\": 2,\n  \"rust_file\": {rust_file},\n  \"segments\": ["
        );
        for (index, segment) in self.segments.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let rust_line = u32::try_from(segment.start_line)
                .ok()
                .and_then(|line| origin.rust_line_at(line))
                .map(|line| format!(",\"rust_line\":{line}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "\n    {{\"slang_start\":{},\"slang_end\":{},\"kernel\":\"{}\",\"construct\":\"{}\",\"ordinal\":{}{rust_line}}}",
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

/// The generated resource-length prologue (`__gpu_len_*`, `__gpu_stride_*`,
/// `GetDimensions`). `__gpu_` names are reserved, so user code cannot match.
fn is_prologue(code: &str) -> bool {
    let declaration = code.starts_with("uint __gpu_len_") || code.starts_with("uint __gpu_stride_");
    (declaration || code.contains(".GetDimensions(__gpu_len_")) && code.ends_with(';')
}

/// The `else` keyword, not an identifier such as `elsewhere` that starts with it.
fn is_else(code: &str) -> bool {
    code == "else" || code.starts_with("else ") || code.starts_with("else{")
}

fn construct_kind(line: &str) -> Option<&'static str> {
    if is_else(line) {
        // Continues the preceding conditional's range.
        None
    } else if line.starts_with("var ") || line.starts_with("uint ") || line.starts_with("int ") {
        Some("local")
    } else if line.starts_with("if (") {
        Some("conditional")
    } else if line.starts_with("for (") {
        Some("loop")
    } else if line == "return;" || line.starts_with("return ") {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape of the real emitter: prologue, a lowered `match`, a loop, an else chain.
    const KERNEL: &str = concat!(
        "// Generated by GUST\n",                                    // 1
        "// @rust kernel: example::run\n",                           // 2
        "[shader(\"compute\")]\n",                                   // 3
        "[numthreads(64, 1, 1)]\n",                                  // 4
        "void gpu_example_run(uint3 id : SV_DispatchThreadID)\n",    // 5
        "{\n",                                                       // 6
        "    uint __gpu_len_out;\n",                                 // 7
        "    uint __gpu_stride_out;\n",                              // 8
        "    out.GetDimensions(__gpu_len_out, __gpu_stride_out);\n", // 9
        "    var i = id.x;\n",                                       // 10
        "    // @gust construct: match #1\n",                        // 11
        "    var __gust_match_1 = pick(i);\n",                       // 12
        "    if (__gust_match_1.isOk)\n",                            // 13
        "    {\n",                                                   // 14
        "        var half = __gust_match_1.value;\n",                // 15
        "        out[i] = half;\n",                                  // 16
        "    }\n",                                                   // 17
        "    else\n",                                                // 18
        "    {\n",                                                   // 19
        "        var error = __gust_match_1.value;\n",               // 20
        "        out[i] = error;\n",                                 // 21
        "    }\n",                                                   // 22
        "    var __gust_end_2 = uint(4);\n",                         // 23
        "    for (var k = uint(0); k < __gust_end_2; k++)\n",        // 24
        "    {\n",                                                   // 25
        "        if ((k == uint(2)))\n",                             // 26
        "        {\n",                                               // 27
        "            continue;\n",                                   // 28
        "        }\n",                                               // 29
        "        else if ((k == uint(3)))\n",                        // 30
        "        {\n",                                               // 31
        "            break;\n",                                      // 32
        "        }\n",                                               // 33
        "        else\n",                                            // 34
        "        {\n",                                               // 35
        "            out[k] = k;\n",                                 // 36
        "        }\n",                                               // 37
        "    }\n",                                                   // 38
        "    return i;\n",                                           // 39
        "}\n",                                                       // 40
        "uint after_kernel() { return 1; }\n",                       // 41
    );

    fn segment(map: &SourceMap, construct: &str, start: usize) -> SourceMapSegment {
        map.segments()
            .iter()
            .find(|segment| segment.construct == construct && segment.start_line == start)
            .unwrap_or_else(|| panic!("no {construct} segment at {start}: {:?}", map.segments()))
            .clone()
    }

    #[test]
    fn kernel_segment_covers_the_entry_point_and_prologue_takes_no_ordinal() {
        let map = SourceMap::from_slang(KERNEL);
        let kernel = segment(&map, "kernel", 2);
        assert_eq!((kernel.end_line, kernel.ordinal), (40, 0));
        assert_eq!(kernel.kernel, "example::run");
        let prologue = segment(&map, "prologue", 7);
        assert_eq!(prologue.end_line, 9);
        // The first user `let` is construct #0 although nine lines precede it.
        let first = segment(&map, "local", 10);
        assert_eq!(first.ordinal, 0);
        assert_eq!(segment(&map, "match", 11).ordinal, 1);
    }

    #[test]
    fn match_is_one_range_over_the_lowered_block() {
        let map = SourceMap::from_slang(KERNEL);
        let lowered = segment(&map, "match", 11);
        assert_eq!((lowered.start_line, lowered.end_line), (11, 22));
        // Scaffolding lines inside the match carry no segment of their own...
        for line in [12, 13, 14, 15, 17, 18, 19, 20, 22] {
            assert_eq!(
                map.origin_at(line).unwrap().construct,
                "match",
                "line {line}"
            );
        }
        // ...while user statements in the arms are nested and more precise.
        assert_eq!(map.origin_at(16).unwrap().construct, "assignment");
        assert_eq!(map.origin_at(21).unwrap().construct, "assignment");
        // The line after the block is no longer in the match.
        assert_eq!(map.origin_at(23).unwrap().construct, "local");
    }

    #[test]
    fn loop_conditional_else_chain_return_and_expression_have_their_own_ranges() {
        let map = SourceMap::from_slang(KERNEL);
        let lp = segment(&map, "loop", 24);
        assert_eq!(lp.end_line, 38);
        let conditional = segment(&map, "conditional", 26);
        // `else if` and `else` continue the same conditional through line 37.
        assert_eq!(conditional.end_line, 37);
        assert_eq!(map.origin_at(30).unwrap().construct, "conditional");
        assert_eq!(map.origin_at(35).unwrap().construct, "conditional");
        assert_eq!(segment(&map, "expression", 28).end_line, 28);
        assert_eq!(segment(&map, "expression", 32).end_line, 32);
        let ret = segment(&map, "return", 39);
        assert_eq!((ret.start_line, ret.end_line), (39, 39));
        // The loop braces map to the loop; statements to the innermost construct.
        assert_eq!(map.origin_at(25).unwrap().construct, "loop");
        assert_eq!(map.origin_at(38).unwrap().construct, "loop");
        assert_eq!(map.origin_at(36).unwrap().construct, "assignment");
    }

    #[test]
    fn origin_at_prefers_innermost_and_falls_back_to_the_kernel() {
        let map = SourceMap::from_slang(KERNEL);
        assert_eq!(map.origin_at(28).unwrap().construct, "expression");
        assert_eq!(map.origin_at(26).unwrap().construct, "conditional");
        // Unsegmented lines inside the entry point: signature and braces.
        for line in [3, 4, 5, 6, 40] {
            let origin = map.origin_at(line).unwrap();
            assert_eq!(
                (origin.construct, origin.kernel.as_str()),
                ("kernel", "example::run"),
                "line {line}"
            );
        }
        // After the entry point closes: the nearest preceding kernel marker.
        let after = map.origin_at(41).unwrap();
        assert_eq!(
            (after.construct, after.kernel.as_str()),
            ("kernel", "example::run")
        );
        // Before any marker nothing is attributed.
        assert!(map.origin_at(1).is_none());
        assert!(
            SourceMap::from_slang("void f() {}\n")
                .origin_at(1)
                .is_none()
        );
    }

    #[test]
    fn each_kernel_has_its_own_ordinals_and_fallback_marker() {
        let source = concat!(
            "// @rust kernel: m::a\n",
            "void a() { }\n",
            "// @rust kernel: m::b\n",
            "void b()\n",
            "{\n",
            "    var x = uint(1);\n",
            "}\n",
            "uint tail;\n",
        );
        let map = SourceMap::from_slang(source);
        let local = segment(&map, "local", 6);
        assert_eq!((local.kernel.as_str(), local.ordinal), ("m::b", 0));
        assert_eq!(map.origin_at(2).unwrap().kernel, "m::a");
        assert_eq!(map.origin_at(8).unwrap().kernel, "m::b");
    }

    #[test]
    fn unterminated_kernel_and_unclosed_marker_are_closed_at_the_end() {
        let source = "// @rust kernel: m::a\nvoid a()\n{\n    // @gust construct: match #0\n    var x = 1;\n";
        let map = SourceMap::from_slang(source);
        assert_eq!(segment(&map, "kernel", 1).end_line, 5);
        // A marker without a block ends at the end of its enclosing block or file.
        assert_eq!(segment(&map, "match", 4).end_line, 5);
        assert_eq!(map.origin_at(5).unwrap().construct, "local");
    }

    #[test]
    fn marker_without_block_ends_at_the_next_marker_at_the_same_depth() {
        let source = concat!(
            "// @rust kernel: m::a\n",
            "void a()\n",
            "{\n",
            "    // @gust construct: match #0\n",
            "    var x = uint(1);\n",
            "    // @gust construct: match #1\n",
            "    var y = uint(2);\n",
            "}\n",
        );
        let map = SourceMap::from_slang(source);
        assert_eq!(segment(&map, "match", 4).end_line, 5);
        assert_eq!(segment(&map, "match", 6).end_line, 7);
    }

    #[test]
    fn identifier_starting_with_else_does_not_continue_a_conditional() {
        let source = concat!(
            "// @rust kernel: m::a\n",                  // 1
            "void a(uint3 id : SV_DispatchThreadID)\n", // 2
            "{\n",                                      // 3
            "    if ((flag > uint(0)))\n",              // 4
            "    {\n",                                  // 5
            "        flag = uint(1);\n",                // 6
            "    }\n",                                  // 7
            "    elsewhere = uint(2);\n",               // 8
            "}\n",                                      // 9
        );
        let map = SourceMap::from_slang(source);
        assert_eq!(segment(&map, "conditional", 4).end_line, 7);
        let user = segment(&map, "assignment", 8);
        assert_eq!((user.start_line, user.end_line), (8, 8));
        assert_eq!(map.origin_at(8).unwrap().construct, "assignment");
    }

    #[test]
    fn braces_inside_a_header_condition_do_not_make_it_a_single_line() {
        let source = concat!(
            "// @rust kernel: m::a\n",                             // 1
            "void a(uint3 id : SV_DispatchThreadID)\n",            // 2
            "{\n",                                                 // 3
            "    if ((sum({ 1, 2 }) > uint(0)))\n",                // 4
            "    {\n",                                             // 5
            "        flag = uint(1);\n",                           // 6
            "    }\n",                                             // 7
            "    for (var k = uint(0); k < len({ 3, 4 }); k++)\n", // 8
            "    {\n",                                             // 9
            "        flag = uint(2);\n",                           // 10
            "    }\n",                                             // 11
            "    flag = uint(3);\n",                               // 12
            "    if ((flag > uint(0))) { flag = uint(4); }\n",     // 13
            "}\n",                                                 // 14
        );
        let map = SourceMap::from_slang(source);
        assert_eq!(segment(&map, "conditional", 4).end_line, 7);
        assert_eq!(segment(&map, "loop", 8).end_line, 11);
        assert_eq!(map.origin_at(5).unwrap().construct, "conditional");
        assert_eq!(map.origin_at(9).unwrap().construct, "loop");
        let after = segment(&map, "assignment", 12);
        assert_eq!((after.start_line, after.end_line), (12, 12));
        // A construct that really sits on one line still ends on it.
        let one_line = segment(&map, "conditional", 13);
        assert_eq!((one_line.start_line, one_line.end_line), (13, 13));
    }

    #[test]
    fn match_ordinal_is_the_markers_own_number_and_bare_return_is_a_return() {
        let source = concat!(
            "// @rust kernel: m::a\n",                  // 1
            "void a(uint3 id : SV_DispatchThreadID)\n", // 2
            "{\n",                                      // 3
            "    var x = uint(0);\n",                   // 4
            "    var y = uint(1);\n",                   // 5
            "    // @gust construct: match #7\n",       // 6
            "    var __gust_match_7 = pick();\n",       // 7
            "    if (__gust_match_7.isOk)\n",           // 8
            "    {\n",                                  // 9
            "        x = uint(1);\n",                   // 10
            "    }\n",                                  // 11
            "    else\n",                               // 12
            "    {\n",                                  // 13
            "        x = uint(2);\n",                   // 14
            "    }\n",                                  // 15
            "    var z = uint(3);\n",                   // 16
            "    return;\n",                            // 17
            "}\n",                                      // 18
        );
        let map = SourceMap::from_slang(source);
        // The map would have numbered this construct 2; the emitter called it 7.
        assert_eq!(segment(&map, "match", 6).ordinal, 7);
        assert!(
            map.json(&SourceOrigin::UNKNOWN)
                .contains("\"construct\":\"match\",\"ordinal\":7")
        );
        // Other constructs keep the per-kernel count, which the marker still advances.
        assert_eq!(segment(&map, "assignment", 10).ordinal, 3);
        assert_eq!(segment(&map, "local", 16).ordinal, 5);
        assert_eq!(segment(&map, "return", 17).end_line, 17);
    }

    #[test]
    fn else_if_header_with_an_array_literal_does_not_close_the_conditional() {
        let source = concat!(
            "// @rust kernel: m::a\n",                                       // 1
            "void a(uint3 id : SV_DispatchThreadID)\n",                      // 2
            "{\n",                                                           // 3
            "    if ((flag > uint(5)))\n",                                   // 4
            "    {\n",                                                       // 5
            "        flag = uint(1);\n",                                     // 6
            "    }\n",                                                       // 7
            "    else if ((sum({ 1, 2 }) > uint(0)))\n",                     // 8
            "    {\n",                                                       // 9
            "        flag = uint(2);\n",                                     // 10
            "    }\n",                                                       // 11
            "    else if ((len({ 3, 4 }) > uint(1))) { flag = uint(5); }\n", // 12
            "    else\n",                                                    // 13
            "    {\n",                                                       // 14
            "        flag = uint(3);\n",                                     // 15
            "    }\n",                                                       // 16
            "    flag = uint(4);\n",                                         // 17
            "}\n",                                                           // 18
        );
        let map = SourceMap::from_slang(source);
        // One conditional from the `if` through the last `else`.
        assert_eq!(segment(&map, "conditional", 4).end_line, 16);
        for line in [8, 9, 11, 13, 14, 16] {
            assert_eq!(
                map.origin_at(line).unwrap().construct,
                "conditional",
                "line {line}"
            );
        }
        assert_eq!(map.origin_at(10).unwrap().construct, "assignment");
        let after = segment(&map, "assignment", 17);
        assert_eq!((after.start_line, after.end_line), (17, 17));
        assert_eq!(map.origin_at(17).unwrap().construct, "assignment");
    }

    #[test]
    fn match_ordinals_are_block_local_and_the_line_range_disambiguates() {
        // Two matches in different blocks of one kernel are both `match #0`.
        let source = concat!(
            "// @rust kernel: m::a\n",                  // 1
            "void a(uint3 id : SV_DispatchThreadID)\n", // 2
            "{\n",                                      // 3
            "    // @gust construct: match #0\n",       // 4
            "    var __gust_match_0 = pick();\n",       // 5
            "    if (__gust_match_0.isOk)\n",           // 6
            "    {\n",                                  // 7
            "        // @gust construct: match #0\n",   // 8
            "        var __gust_match_0 = pick();\n",   // 9
            "        if (__gust_match_0.isOk)\n",       // 10
            "        {\n",                              // 11
            "            x = uint(1);\n",               // 12
            "        }\n",                              // 13
            "        else\n",                           // 14
            "        {\n",                              // 15
            "            x = uint(2);\n",               // 16
            "        }\n",                              // 17
            "    }\n",                                  // 18
            "    else\n",                               // 19
            "    {\n",                                  // 20
            "        x = uint(3);\n",                   // 21
            "    }\n",                                  // 22
            "}\n",                                      // 23
        );
        let map = SourceMap::from_slang(source);
        let outer = segment(&map, "match", 4);
        let inner = segment(&map, "match", 8);
        assert_eq!((outer.ordinal, inner.ordinal), (0, 0));
        assert_eq!((outer.start_line, outer.end_line), (4, 22));
        assert_eq!((inner.start_line, inner.end_line), (8, 17));
        // Same label, different range: the reported line picks the right one.
        assert_eq!(map.origin_at(19).unwrap().start_line, 4);
        assert_eq!(map.origin_at(14).unwrap().start_line, 8);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "unparsable match marker number")]
    fn unparsable_match_marker_number_is_an_internal_error_in_debug() {
        let source = concat!(
            "// @rust kernel: m::a\n",
            "void a()\n",
            "{\n",
            "    // @gust construct: match #x\n",
            "    var y = uint(1);\n",
            "}\n",
        );
        let _ = SourceMap::from_slang(source);
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn unparsable_match_marker_number_falls_back_to_the_map_ordinal_in_release() {
        let source = concat!(
            "// @rust kernel: m::a\n",
            "void a()\n",
            "{\n",
            "    var x = uint(0);\n",
            "    // @gust construct: match #x\n",
            "    var y = uint(1);\n",
            "}\n",
        );
        let map = SourceMap::from_slang(source);
        assert_eq!(segment(&map, "match", 5).ordinal, 1);
    }

    #[test]
    fn sidecar_json_has_stable_schema_fields_and_kinds() {
        let map = SourceMap::from_slang(KERNEL);
        let json = map.json(&SourceOrigin::UNKNOWN);
        assert!(
            json.starts_with(
                "{\n  \"schema_version\": 2,\n  \"rust_file\": null,\n  \"segments\": ["
            )
        );
        assert!(json.ends_with("\n  ]\n}\n"));
        assert!(json.contains(
            "{\"slang_start\":2,\"slang_end\":40,\"kernel\":\"example::run\",\"construct\":\"kernel\",\"ordinal\":0}"
        ));
        assert!(json.contains(
            "{\"slang_start\":7,\"slang_end\":9,\"kernel\":\"example::run\",\"construct\":\"prologue\",\"ordinal\":0}"
        ));
        for kind in [
            "kernel",
            "prologue",
            "local",
            "conditional",
            "loop",
            "match",
            "assignment",
            "return",
            "expression",
        ] {
            assert!(
                json.contains(&format!("\"construct\":\"{kind}\"")),
                "missing {kind}: {json}"
            );
        }
        assert!(!json.contains("rust_line"), "unknown origin: {json}");
        assert_eq!(
            json,
            SourceMap::from_slang(KERNEL).json(&SourceOrigin::UNKNOWN),
            "deterministic"
        );
    }

    #[test]
    fn sidecar_json_v2_carries_the_rust_file_and_line_of_mapped_segments_only() {
        use crate::LineSpan;
        // KERNEL: `var i = id.x;` is Slang line 10, the match scaffolding starts at 11
        // and the loop spans 23 to 38. Lines 2 to 9 (marker, attributes, prologue)
        // have no record.
        const LINES: &[LineSpan] = &[
            LineSpan {
                slang_start: 10,
                slang_end: 10,
                rust_line: 41,
            },
            LineSpan {
                slang_start: 23,
                slang_end: 38,
                rust_line: 50,
            },
            LineSpan {
                slang_start: 26,
                slang_end: 29,
                rust_line: 52,
            },
        ];
        let origin = SourceOrigin {
            file: "crates/demo/src/lib.rs",
            lines: LINES,
        };
        let json = SourceMap::from_slang(KERNEL).json(&origin);
        assert!(
            json.starts_with(
                "{\n  \"schema_version\": 2,\n  \"rust_file\": \"crates/demo/src/lib.rs\",\n  \"segments\": ["
            ),
            "{json}"
        );
        // Mapped: the key follows the ordinal.
        assert!(
            json.contains(
                "{\"slang_start\":10,\"slang_end\":10,\"kernel\":\"example::run\",\"construct\":\"local\",\"ordinal\":0,\"rust_line\":41}"
            ),
            "{json}"
        );
        // The nested conditional inside the loop gets the innermost record.
        let map = SourceMap::from_slang(KERNEL);
        let inner = segment(&map, "conditional", 26);
        assert!(
            json.contains(&format!(
                "{{\"slang_start\":26,\"slang_end\":{},\"kernel\":\"example::run\",\"construct\":\"conditional\",\"ordinal\":{},\"rust_line\":52}}",
                inner.end_line, inner.ordinal
            )),
            "{json}"
        );
        let outer = segment(&map, "loop", 24);
        assert!(
            json.contains(&format!(
                "{{\"slang_start\":24,\"slang_end\":{},\"kernel\":\"example::run\",\"construct\":\"loop\",\"ordinal\":{},\"rust_line\":50}}",
                outer.end_line, outer.ordinal
            )),
            "{json}"
        );
        // Unmapped segments omit the key entirely.
        assert!(
            json.contains(
                "{\"slang_start\":2,\"slang_end\":40,\"kernel\":\"example::run\",\"construct\":\"kernel\",\"ordinal\":0}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"slang_start\":7,\"slang_end\":9,\"kernel\":\"example::run\",\"construct\":\"prologue\",\"ordinal\":0}"
            ),
            "{json}"
        );
        assert_eq!(json, SourceMap::from_slang(KERNEL).json(&origin));
    }
}
