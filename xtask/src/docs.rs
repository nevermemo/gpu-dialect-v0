use std::{collections::BTreeSet, path::Path};

use crate::{
    process::{CheckRecord, report_check},
    workspace::{project_files, read_text},
};

const DECISIONS_PATH: &str = "docs/DECISIONS.md";

/// `cargo xtask docs-check`: relative Markdown links, merge-conflict markers and the decision
/// log. Needs only git; also part of `check-fast` and CI.
pub(crate) fn docs_check(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<(), String> {
    report_check(checks, "docs-check", check_project(root))
}

/// Returns a summary on success, or one line per problem.
fn check_project(root: &Path) -> Result<String, Vec<String>> {
    let files = project_files(root).map_err(|error| vec![error])?;
    let known: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let mut problems = Vec::new();
    let (mut markdown, mut links, mut text_files) = (0usize, 0usize, 0usize);
    for relative in &files {
        let text = match read_text(&root.join(relative)) {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(error) => {
                problems.push(format!("{relative}: could not be read: {error}"));
                continue;
            }
        };
        text_files += 1;
        problems.extend(conflict_marker_problems(relative, &text));
        if is_markdown(relative) {
            markdown += 1;
            for link in markdown_links(&text) {
                match check_link(relative, &link.target, &known) {
                    LinkCheck::Skipped => {}
                    LinkCheck::Resolved => links += 1,
                    LinkCheck::Broken(reason) => problems.push(format!(
                        "{relative}:{}: link `{}` {reason}",
                        link.line, link.target
                    )),
                }
            }
        }
    }
    let mut decisions = None;
    match read_text(&root.join(DECISIONS_PATH)) {
        Ok(Some(text)) => match decision_ids(&text) {
            Ok(found) => decisions = Some(found),
            Err(found) => problems.extend(found.iter().map(|p| format!("{DECISIONS_PATH}: {p}"))),
        },
        _ => problems.push(format!("{DECISIONS_PATH}: missing or not readable text")),
    }
    let (Some(decisions), true) = (decisions, problems.is_empty()) else {
        return Err(problems);
    };
    Ok(format!(
        "Docs check passed: {markdown} Markdown files, {links} relative links resolve to project files (anchors, external links and HTML attributes are not checked); {text_files} text files have no merge-conflict markers; {DECISIONS_PATH} has {} ids D01..D{:02}, unique and increasing, next id D{:02}.\n",
        decisions.count,
        decisions.largest,
        decisions.largest + 1
    ))
}

/// True for a path with a `.md` extension in any case (`README.MD` is Markdown too).
pub(crate) fn is_markdown(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("md"))
}

pub(crate) struct Link {
    pub(crate) line: usize,
    pub(crate) target: String,
}

/// Inline links and images (`[text](target)`) and reference definitions (`[label]: target`)
/// outside fenced code blocks and inline code spans. Targets come back as written, without
/// angle brackets or a title.
pub(crate) fn markdown_links(text: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (index, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        if let Some(marker) = fence_marker(trimmed) {
            match fence {
                None => fence = Some(marker),
                // A closing fence has no info string.
                Some((kind, length))
                    if marker.0 == kind
                        && marker.1 >= length
                        && trimmed[marker.1..].trim().is_empty() =>
                {
                    fence = None
                }
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let line = blank_code_spans(raw);
        if let Some(target) = reference_definition(&line) {
            links.push(Link {
                line: index + 1,
                target,
            });
        }
        let mut rest = line.as_str();
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            if let Some(target) = link_destination(rest) {
                links.push(Link {
                    line: index + 1,
                    target,
                });
            }
        }
    }
    links
}

/// The fence character and length when `line` opens or closes a fenced code block.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let first = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = line.chars().take_while(|c| *c == first).count();
    (length >= 3).then_some((first, length))
}

/// Replaces every inline code span with spaces, so links inside code are not links.
fn blank_code_spans(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| **c == '`').count();
        // The closing run must have exactly the same length as the opening one.
        let mut j = i + run;
        let mut close = None;
        while j < chars.len() {
            if chars[j] == '`' {
                let length = chars[j..].iter().take_while(|c| **c == '`').count();
                if length == run {
                    close = Some(j + length);
                    break;
                }
                j += length;
            } else {
                j += 1;
            }
        }
        match close {
            Some(end) => {
                out.extend(std::iter::repeat_n(' ', end - i));
                i = end;
            }
            None => {
                out.extend(std::iter::repeat_n('`', run));
                i += run;
            }
        }
    }
    out
}

/// `[label]: target` at the start of a line (up to three spaces of indentation).
fn reference_definition(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = line[indent..].strip_prefix('[')?;
    let close = rest.find("]:")?;
    // `[^1]: text` defines a footnote; its text is not a link target.
    if rest[..close].is_empty() || rest[..close].contains(']') || rest.starts_with('^') {
        return None;
    }
    link_destination(rest[close + 2..].trim_start())
}

/// The destination at the start of `rest` (the text after `](`): `<...>` or a run without
/// spaces whose parentheses balance. `None` for an empty destination.
fn link_destination(rest: &str) -> Option<String> {
    if let Some(inner) = rest.strip_prefix('<') {
        let end = inner.find('>')?;
        return Some(inner[..end].to_owned()).filter(|target| !target.is_empty());
    }
    let mut depth = 0usize;
    let mut end = rest.len();
    for (offset, c) in rest.char_indices() {
        match c {
            c if c.is_whitespace() => {
                end = offset;
                break;
            }
            '(' => depth += 1,
            ')' if depth == 0 => {
                end = offset;
                break;
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    Some(rest[..end].to_owned()).filter(|target| !target.is_empty())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinkCheck {
    /// An anchor-only, external, or empty link; not checked.
    Skipped,
    Resolved,
    Broken(String),
}

/// Checks one link written in `from` (a repository-relative path). `known` holds every
/// project file. Anchors (`#...`) and queries are dropped; external links are not fetched.
pub(crate) fn check_link(from: &str, target: &str, known: &BTreeSet<&str>) -> LinkCheck {
    let target = target.trim();
    if target.is_empty() || target.starts_with('#') || target.starts_with("//") {
        return LinkCheck::Skipped;
    }
    if has_scheme(target) {
        return LinkCheck::Skipped;
    }
    let path = target.split(['#', '?']).next().unwrap_or(target);
    if path.is_empty() {
        return LinkCheck::Skipped;
    }
    let path = path.replace("%20", " ");
    let mut parts: Vec<&str> = Vec::new();
    if !path.starts_with('/') {
        parts.extend(from.split('/').filter(|part| !part.is_empty()));
        parts.pop();
    }
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return LinkCheck::Broken("leaves the repository".to_owned());
                }
            }
            part => parts.push(part),
        }
    }
    let resolved = parts.join("/");
    if known.contains(resolved.as_str()) {
        LinkCheck::Resolved
    } else {
        LinkCheck::Broken(format!("does not resolve to a project file (`{resolved}`)"))
    }
}

/// True for a `scheme:` prefix such as `https:` or `mailto:`. A relative link whose first
/// segment contains `:` would need `./` in front, so this does not hide real file links.
fn has_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// Merge-conflict markers: a line that starts with seven `<` or seven `>` followed by a
/// space or the end of the line, or a line of exactly seven `=`. In Markdown a lone
/// `=======` line is also a heading underline, so there it counts only when the file has
/// another marker.
pub(crate) fn conflict_marker_problems(path: &str, text: &str) -> Vec<String> {
    let is_marker = |line: &str, kind: char| {
        let run = line.chars().take_while(|c| *c == kind).count();
        run == 7 && line[7..].chars().next().is_none_or(|c| c == ' ')
    };
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let open_or_close = |line: &str| is_marker(line, '<') || is_marker(line, '>');
    let has_other = lines.iter().any(|line| open_or_close(line));
    let separator_counts = !is_markdown(path) || has_other;
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            open_or_close(line) || (separator_counts && line.trim_end() == "=======")
        })
        .map(|(index, line)| {
            format!(
                "{path}:{}: merge-conflict marker `{}`",
                index + 1,
                line.trim_end()
            )
        })
        .collect()
}

#[derive(Debug)]
pub(crate) struct DecisionIds {
    pub(crate) count: usize,
    pub(crate) largest: u32,
}

/// Checks the decision table of `docs/DECISIONS.md`: ids `D<digits>` unique (also when the
/// repeat is not next to the first) and increasing in table order, and the header's "Next id"
/// one more than the largest id.
pub(crate) fn decision_ids(text: &str) -> Result<DecisionIds, Vec<String>> {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    let mut previous: Option<u32> = None;
    let mut largest = 0u32;
    let mut count = 0usize;
    for (index, line) in text.lines().enumerate() {
        let Some(id) = table_row_id(line) else {
            continue;
        };
        let number = index + 1;
        count += 1;
        match previous {
            _ if !seen.insert(id) => {
                problems.push(format!("line {number}: duplicate decision id D{id:02}"))
            }
            Some(last) if id < last => problems.push(format!(
                "line {number}: D{id:02} is out of order after D{last:02}; ids must increase"
            )),
            _ => {}
        }
        previous = Some(id);
        largest = largest.max(id);
    }
    if count == 0 {
        problems.push("no decision rows (`| D01 | ...`) found".to_owned());
    }
    match next_id(text) {
        None => problems.push("header has no `Next id: **Dnn**`".to_owned()),
        Some(next) if count > 0 && next != largest + 1 => problems.push(format!(
            "header says Next id D{next:02}, but the largest id is D{largest:02} (expected D{:02})",
            largest + 1
        )),
        Some(_) => {}
    }
    if problems.is_empty() {
        Ok(DecisionIds { count, largest })
    } else {
        Err(problems)
    }
}

/// The number of a `| D12 | ...` table row.
fn table_row_id(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("| D")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits < 2 || !rest[digits..].trim_start().starts_with('|') {
        return None;
    }
    rest[..digits].parse().ok()
}

/// The number in `Next id: **D28**`.
fn next_id(text: &str) -> Option<u32> {
    let rest = text.split("Next id: **D").nth(1)?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && rest[digits..].starts_with("**"))
        .then(|| rest[..digits].parse().ok())
        .flatten()
}

#[cfg(test)]
#[path = "tests/docs.rs"]
mod docs_tests;
