//! Persisted hunks have no full-file baseline. Never fill their gaps from disk.
use super::*;

impl Comparison {
    pub(crate) fn from_patch(patch: &str) -> Result<Self, &'static str> {
        let rows = parse(patch);
        if rows.len() > 40000 || patch.contains('\0') {
            return Err("Historical patch exceeds the text comparison limit");
        }
        let mut lines = Vec::new();
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        let mut remaining = None;
        let mut previous_end = None;
        for row in rows {
            if row.kind == Kind::Hunk {
                flush(&mut lines, &mut removed, &mut added);
                if remaining.is_some_and(|counts| counts != (0usize, 0usize)) {
                    return Err("Incomplete historical hunk");
                }
                let hunk = header(&row.text).ok_or("Invalid historical hunk")?;
                let start = (
                    hunk.old.saturating_sub(usize::from(hunk.old_count != 0)),
                    hunk.new.saturating_sub(usize::from(hunk.new_count != 0)),
                );
                if previous_end.map_or(start != (0, 0), |end| end != start) {
                    lines.push(AlignedLine { before: None, after: None, changed: false });
                }
                remaining = Some((hunk.old_count, hunk.new_count));
                previous_end = Some((
                    start.0.checked_add(hunk.old_count).ok_or("Invalid historical range")?,
                    start.1.checked_add(hunk.new_count).ok_or("Invalid historical range")?,
                ));
                continue;
            }
            if !matches!(row.kind, Kind::Context | Kind::Removed | Kind::Added) {
                continue;
            }
            let counts = remaining.as_mut().ok_or("Missing historical hunk header")?;
            let text = patch
                .get(row.range.clone())
                .and_then(|line| line.get(1..))
                .ok_or("Invalid historical text")?
                .trim_end_matches(['\r', '\n']);
            let before = if let Some(number) = row.old {
                counts.0 = counts.0.checked_sub(1).ok_or("Invalid historical line count")?;
                Some(FileLine {
                    number: number as usize,
                    text: text.into(),
                    patch: Some(row.range.clone()),
                })
            } else {
                None
            };
            let after = if let Some(number) = row.new {
                counts.1 = counts.1.checked_sub(1).ok_or("Invalid historical line count")?;
                Some(FileLine {
                    number: number as usize,
                    text: text.into(),
                    patch: Some(row.range),
                })
            } else {
                None
            };
            match row.kind {
                Kind::Context => {
                    flush(&mut lines, &mut removed, &mut added);
                    lines.push(AlignedLine { before, after, changed: false });
                },
                Kind::Removed => removed.extend(before),
                Kind::Added => added.extend(after),
                _ => {},
            }
        }
        flush(&mut lines, &mut removed, &mut added);
        if remaining != Some((0, 0)) {
            return Err("No complete text hunks in this historical record");
        }
        lines.push(AlignedLine { before: None, after: None, changed: false });
        Ok(Self { lines })
    }
}
