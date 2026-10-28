//! The same byte ranges drive visual gutters, annotations and whole-hunk staging.
use std::ops::Range;
mod aligned;
pub(crate) use aligned::{AlignedLine, Comparison};

struct Header {
    old: usize,
    old_count: usize,
    new: usize,
    new_count: usize,
}
fn header(text: &str) -> Option<Header> {
    fn part(value: &str, prefix: char) -> Option<(usize, usize)> {
        let value = value.strip_prefix(prefix)?;
        let (start, count) = value.split_once(',').map_or((value, "1"), |parts| parts);
        Some((start.parse().ok()?, count.parse().ok()?))
    }
    let mut parts = text.strip_prefix("@@ ")?.split_whitespace();
    let (old, old_count) = part(parts.next()?, '-')?;
    let (new, new_count) = part(parts.next()?, '+')?;
    if parts.next()? != "@@" {
        return None;
    }
    Some(Header { old, old_count, new, new_count })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Metadata,
    Hunk,
    Context,
    Added,
    Removed,
}
pub(crate) struct PatchLine {
    pub kind: Kind,
    pub range: Range<usize>,
    pub text: String,
    pub old: Option<u64>,
    pub new: Option<u64>,
    hunk: Option<usize>,
}

pub(crate) fn parse(text: &str) -> Vec<PatchLine> {
    let mut offset = 0;
    let mut position = None;
    let mut hunk = None;
    let mut next_hunk = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let range = offset..offset + line.len();
            offset = range.end;
            let display = line.trim_end_matches(['\r', '\n']);
            let mut kind = Kind::Metadata;
            let (mut old, mut new) = (None, None);
            if display.starts_with("@@ ") {
                position = header(display).map(|header| (header.old as u64, header.new as u64));
                hunk = position.map(|_| next_hunk);
                next_hunk += 1;
                kind = Kind::Hunk;
            } else if display.starts_with("diff ") {
                position = None;
                hunk = None;
            } else if let Some((before, after)) = position.as_mut() {
                match display.as_bytes().first() {
                    Some(b'+') => {
                        kind = Kind::Added;
                        new = Some(*after);
                        *after = after.saturating_add(1);
                    },
                    Some(b'-') => {
                        kind = Kind::Removed;
                        old = Some(*before);
                        *before = before.saturating_add(1);
                    },
                    Some(b' ') => {
                        kind = Kind::Context;
                        old = Some(*before);
                        new = Some(*after);
                        *before = before.saturating_add(1);
                        *after = after.saturating_add(1);
                    },
                    _ => {},
                }
            }
            PatchLine { kind, range, text: display.replace('\t', "    "), old, new, hunk }
        })
        .collect()
}

pub(crate) fn selected_hunk(rows: &[PatchLine], selection: Range<usize>) -> Option<Range<usize>> {
    if selection.is_empty() {
        return None;
    }
    let mut selected = rows
        .iter()
        .filter(|row| row.range.start < selection.end && row.range.end > selection.start);
    let hunk = selected.next()?.hunk?;
    if selected.any(|row| row.hunk != Some(hunk)) {
        return None;
    }
    let mut rows = rows.iter().filter(|row| row.hunk == Some(hunk));
    let first = rows.next()?;
    Some(first.range.start..rows.last().map_or(first.range.end, |row| row.range.end))
}

pub(crate) fn supports_hunks(rows: &[PatchLine]) -> bool {
    let Some(first) = rows.iter().find(|row| row.kind == Kind::Hunk) else { return false };
    let header: Vec<_> =
        rows.iter().take_while(|row| row.range.start < first.range.start).collect();
    header.first().is_some_and(|row| row.text.starts_with("diff --git "))
        && header.iter().filter(|row| row.text.starts_with("diff --git ")).count() == 1
        && header.iter().any(|row| {
            row.text.starts_with("index ")
                && (row.text.ends_with(" 100644") || row.text.ends_with(" 100755"))
        })
        && !header.iter().any(|row| {
            [
                "old mode ",
                "new mode ",
                "new file mode ",
                "deleted file mode ",
                "rename ",
                "copy ",
                "Binary files ",
                "GIT binary patch",
            ]
            .iter()
            .any(|prefix| row.text.starts_with(prefix))
        })
        && rows.iter().filter(|row| row.text.starts_with("diff --git ")).count() == 1
}

pub(super) fn extract(text: &str, selection: Range<usize>) -> Option<String> {
    let rows = parse(text);
    if !supports_hunks(&rows) {
        return None;
    }
    let range = selected_hunk(&rows, selection)?;
    let first = rows.iter().find(|row| row.kind == Kind::Hunk)?.range.start;
    Some(format!("{}{}", text.get(..first)?, text.get(range)?))
}
