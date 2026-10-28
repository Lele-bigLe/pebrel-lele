use super::*;
mod historical;

pub(crate) struct FileLine {
    pub number: usize,
    pub text: String,
    pub patch: Option<Range<usize>>,
}
pub(crate) struct AlignedLine {
    pub before: Option<FileLine>,
    pub after: Option<FileLine>,
    pub changed: bool,
}
pub(crate) struct Comparison {
    pub lines: Vec<AlignedLine>,
}

fn flush(lines: &mut Vec<AlignedLine>, old: &mut Vec<FileLine>, new: &mut Vec<FileLine>) {
    let mut old = old.drain(..);
    let mut new = new.drain(..);
    loop {
        let (before, after) = (old.next(), new.next());
        if before.is_none() && after.is_none() {
            break;
        }
        lines.push(AlignedLine { before, after, changed: true });
    }
}

impl Comparison {
    pub(crate) fn build(before: &str, patch: &str) -> Result<Self, &'static str> {
        if before.contains('\0')
            || patch.contains("GIT binary patch")
            || patch.lines().any(|line| line.starts_with("Binary files "))
        {
            return Err("Binary changes are available in the raw diff only");
        }
        let rows = parse(patch);
        if rows.iter().filter(|row| row.text.starts_with("diff ")).count() > 1 {
            return Err("This patch contains more than one file");
        }
        let source: Vec<_> = before.lines().collect();
        if source.len() > 20000 || rows.len() > 40000 {
            return Err("Comparison exceeds the line limit");
        }
        let (mut old_cursor, mut new_cursor) = (0usize, 0usize);
        let mut end = None;
        let mut lines = Vec::new();
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        let invalid = "The Git baseline does not match the displayed patch; refresh";
        for row in rows {
            if row.kind == Kind::Hunk {
                flush(&mut lines, &mut removed, &mut added);
                if end.is_some_and(|end| end != (old_cursor, new_cursor)) {
                    return Err(invalid);
                }
                let header = header(&row.text).ok_or(invalid)?;
                let target = header.old.saturating_sub(usize::from(header.old_count != 0));
                if target < old_cursor || target > source.len() {
                    return Err(invalid);
                }
                while old_cursor < target {
                    let text = source[old_cursor].to_owned();
                    lines.push(AlignedLine {
                        before: Some(FileLine {
                            number: old_cursor + 1,
                            text: text.clone(),
                            patch: None,
                        }),
                        after: Some(FileLine { number: new_cursor + 1, text, patch: None }),
                        changed: false,
                    });
                    old_cursor += 1;
                    new_cursor += 1;
                }
                if new_cursor != header.new.saturating_sub(usize::from(header.new_count != 0)) {
                    return Err(invalid);
                }
                end = Some((
                    target.checked_add(header.old_count).ok_or(invalid)?,
                    new_cursor.checked_add(header.new_count).ok_or(invalid)?,
                ));
                continue;
            }
            if !matches!(row.kind, Kind::Context | Kind::Removed | Kind::Added) {
                continue;
            }
            if end.is_none() {
                return Err(invalid);
            }
            let text = patch
                .get(row.range.clone())
                .and_then(|line| line.get(1..))
                .ok_or(invalid)?
                .trim_end_matches(['\r', '\n']);
            if matches!(row.kind, Kind::Context | Kind::Removed)
                && (source.get(old_cursor).copied() != Some(text)
                    || row.old != Some(old_cursor as u64 + 1))
            {
                return Err(invalid);
            }
            if matches!(row.kind, Kind::Context | Kind::Added)
                && row.new != Some(new_cursor as u64 + 1)
            {
                return Err(invalid);
            }
            match row.kind {
                Kind::Context => {
                    flush(&mut lines, &mut removed, &mut added);
                    lines.push(AlignedLine {
                        before: Some(FileLine {
                            number: old_cursor + 1,
                            text: text.into(),
                            patch: Some(row.range.clone()),
                        }),
                        after: Some(FileLine {
                            number: new_cursor + 1,
                            text: text.into(),
                            patch: Some(row.range),
                        }),
                        changed: false,
                    });
                    old_cursor += 1;
                    new_cursor += 1;
                },
                Kind::Removed => {
                    removed.push(FileLine {
                        number: old_cursor + 1,
                        text: text.into(),
                        patch: Some(row.range),
                    });
                    old_cursor += 1;
                },
                Kind::Added => {
                    added.push(FileLine {
                        number: new_cursor + 1,
                        text: text.into(),
                        patch: Some(row.range),
                    });
                    new_cursor += 1;
                },
                _ => {},
            }
            if lines.len() + removed.len() + added.len() > 40000 {
                return Err("Comparison exceeds the line limit");
            }
        }
        flush(&mut lines, &mut removed, &mut added);
        if end.is_some_and(|end| end != (old_cursor, new_cursor)) {
            return Err(invalid);
        }
        while old_cursor < source.len() {
            let text = source[old_cursor].to_owned();
            lines.push(AlignedLine {
                before: Some(FileLine { number: old_cursor + 1, text: text.clone(), patch: None }),
                after: Some(FileLine { number: new_cursor + 1, text, patch: None }),
                changed: false,
            });
            old_cursor += 1;
            new_cursor += 1;
        }
        Ok(Self { lines })
    }
}
