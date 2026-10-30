//! Semantic word changes, independent of syntax highlighting and terminal styles.
use crate::model::{DiffHunk, DiffLine, LineOrigin};
use regex::Regex;
use std::{collections::HashMap, ops::Range};

#[derive(Debug)]
pub struct WordDiff {
    regex: Regex,
    hunks: HashMap<(usize, usize), Vec<Vec<Range<usize>>>>,
}
impl WordDiff {
    pub fn new(regex: Regex) -> Self {
        Self {
            regex,
            hunks: HashMap::new(),
        }
    }
    pub fn clear(&mut self) {
        self.hunks.clear();
    }
    pub fn ranges(&self, file: usize, hunk: usize, line: usize) -> &[Range<usize>] {
        self.hunks
            .get(&(file, hunk))
            .and_then(|lines| lines.get(line))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn prepare(&mut self, file: usize, index: usize, hunk: &DiffHunk) {
        if !self.hunks.contains_key(&(file, index)) {
            self.hunks
                .insert((file, index), changed_lines(&self.regex, &hunk.lines));
        }
    }
}
// Work is capped before matching/diffing; oversized pairs retain ordinary coloring.
const MAX_LINE_BYTES: usize = 16 * 1024;
const MAX_TOKENS: usize = 2048;
const MAX_HUNK_BYTES: usize = 1024 * 1024;
const MAX_HUNK_COMPARISONS: usize = 1_000_000;

fn changed_lines(regex: &Regex, lines: &[DiffLine]) -> Vec<Vec<Range<usize>>> {
    let mut changed = vec![vec![]; lines.len()];
    let mut bytes_left = MAX_HUNK_BYTES;
    let mut comparisons_left = MAX_HUNK_COMPARISONS;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].origin != LineOrigin::Deletion {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].origin == LineOrigin::Deletion {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && lines[i].origin == LineOrigin::Addition {
            i += 1;
        }
        // Match the existing side-by-side renderer's positional pairing. Never
        // compare across context or emphasize an unmatched whole-line change.
        for offset in 0..(add_start - del_start).min(i - add_start) {
            let old_index = del_start + offset;
            let new_index = add_start + offset;
            let old = &lines[old_index].content;
            let new = &lines[new_index].content;
            if old.len() > MAX_LINE_BYTES || new.len() > MAX_LINE_BYTES {
                continue;
            }
            let bytes = old.len() + new.len();
            if bytes > bytes_left {
                return changed;
            }
            bytes_left -= bytes;
            if old == new {
                continue;
            }
            let Some(old_tokens) = tokens(regex, old) else {
                continue;
            };
            let Some(new_tokens) = tokens(regex, new) else {
                continue;
            };
            let comparisons = old_tokens.len().max(1) * new_tokens.len().max(1);
            if comparisons > comparisons_left {
                continue;
            }
            comparisons_left -= comparisons;
            let old_text: Vec<_> = old_tokens.iter().map(|range| &old[range.clone()]).collect();
            let new_text: Vec<_> = new_tokens.iter().map(|range| &new[range.clone()]).collect();
            for op in similar::capture_diff_slices(similar::Algorithm::Myers, &old_text, &new_text)
            {
                if op.tag() != similar::DiffTag::Equal {
                    changed[old_index].extend(old_tokens[op.old_range()].iter().cloned());
                    changed[new_index].extend(new_tokens[op.new_range()].iter().cloned());
                }
            }
        }
    }
    changed
}

fn tokens(regex: &Regex, text: &str) -> Option<Vec<Range<usize>>> {
    let tokens: Vec<_> = regex
        .find_iter(text)
        // Zero-width matches contain no displayable bytes. Ignoring them also
        // permits mixed empty/nonempty patterns without special regex rewriting.
        .filter(|m| !m.is_empty())
        .take(MAX_TOKENS + 1)
        .map(|m| m.range())
        .collect();
    (tokens.len() <= MAX_TOKENS).then_some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(origin: LineOrigin, content: &str) -> DiffLine {
        DiffLine {
            origin,
            content: content.into(),
            old_lineno: None,
            new_lineno: None,
            highlighted_spans: None,
        }
    }
    fn pair(pattern: &str, old: &str, new: &str) -> Vec<Vec<Range<usize>>> {
        changed_lines(
            &Regex::new(pattern).unwrap(),
            &[
                line(LineOrigin::Deletion, old),
                line(LineOrigin::Addition, new),
            ],
        )
    }
    #[test]
    fn whitespace_tokens_preserve_utf8_offsets_and_ignore_whitespace() {
        assert_eq!(
            pair(r"\S+", "let café = old;", "let café = new;"),
            vec![vec![12..16], vec![12..16]]
        );
        assert_eq!(
            pair(r"\S+", "a  b\t", "a b "),
            vec![Vec::<Range<usize>>::new(), Vec::new()]
        );
        assert_eq!(
            pair(r"\S+", "😀 cat", "😀 dog"),
            vec![vec![5..8], vec![5..8]]
        );
    }
    #[test]
    fn regex_modes_preserve_punctuation_and_character_granularity() {
        assert_eq!(
            pair(r"[[:alnum:]_]+|[^[:space:]]", "call(old);", "call(new);"),
            vec![vec![5..8], vec![5..8]]
        );
        assert_eq!(
            pair(r"[^[:space:]]", "café", "cafè"),
            vec![vec![3..5], vec![3..5]]
        );
        assert_eq!(
            pair(r"[^[:space:]]", "a+b", "a-b"),
            vec![vec![1..2], vec![1..2]]
        );
    }
    #[test]
    fn zero_width_matches_are_ignored() {
        assert_eq!(
            pair(r"", "old", "new"),
            vec![Vec::<Range<usize>>::new(), Vec::new()]
        );
        assert_eq!(pair(r"\w+|\b", "old", "new"), vec![vec![0..3], vec![0..3]]);
    }
    #[test]
    fn pairs_positionally_only_with_adjacent_additions() {
        use LineOrigin::*;
        let lines = [
            line(Deletion, "a old"),
            line(Deletion, "unpaired"),
            line(Addition, "a new"),
            line(Context, "context"),
            line(Addition, "addition"),
            line(Deletion, "deletion"),
        ];
        assert_eq!(
            changed_lines(&Regex::new(r"\S+").unwrap(), &lines),
            vec![vec![2..5], vec![], vec![2..5], vec![], vec![], vec![]]
        );
    }
    #[test]
    fn bounds_fall_back_without_emphasis() {
        let long = "a".repeat(20_000);
        assert_eq!(
            pair(r".", &long, "b"),
            vec![Vec::<Range<usize>>::new(), Vec::new()]
        );
        assert_eq!(
            pair(r".", &"a".repeat(1500), &"b".repeat(1500)),
            vec![Vec::<Range<usize>>::new(), Vec::new()]
        );
    }
}
