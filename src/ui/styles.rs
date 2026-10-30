use ratatui::style::{Color, Modifier, Style};

use crate::model::LineOrigin;
use crate::theme::Theme;

pub fn selected_style(theme: &Theme) -> Style {
    Style::default().bg(theme.bg_highlight).fg(theme.fg_primary)
}

pub fn dim_style(theme: &Theme) -> Style {
    Style::default().fg(theme.fg_dim)
}

pub fn diff_add_style(theme: &Theme) -> Style {
    Style::default().fg(theme.diff_add).bg(theme.diff_add_bg)
}

pub fn diff_del_style(theme: &Theme) -> Style {
    Style::default().fg(theme.diff_del).bg(theme.diff_del_bg)
}

pub fn diff_context_style(theme: &Theme) -> Style {
    Style::default().fg(theme.diff_context)
}

/// Live background a syntax-highlighted diff span should show for a line of
/// the given origin. `DiffLine.highlighted_spans` bakes in whatever
/// `syntax_add_bg`/`syntax_del_bg` was active when the diff was loaded (or
/// last rehighlighted); this lets renderers override that per-frame from the
/// currently active theme instead of trusting the cached span's own
/// background, so add/del backgrounds never lag behind the active theme even
/// if a rehighlight was skipped or missed. `None` for context lines, which
/// never carry a baked-in background (`syntax::apply_diff_background`
/// intentionally excludes them).
pub fn diff_syntax_bg(theme: &Theme, origin: LineOrigin) -> Option<Color> {
    match origin {
        LineOrigin::Addition => Some(theme.syntax_add_bg),
        LineOrigin::Deletion => Some(theme.syntax_del_bg),
        LineOrigin::Context => None,
    }
}

/// Apply `diff_syntax_bg` to `style`, leaving it untouched when there is no
/// live override for `origin` (context lines).
pub fn patch_highlighted_span_bg(style: Style, theme: &Theme, origin: LineOrigin) -> Style {
    match diff_syntax_bg(theme, origin) {
        Some(bg) => style.bg(bg),
        None => style,
    }
}

/// Overlay semantic UTF-8 byte ranges after normal diff backgrounds, before
/// search/selection and layout. The range cache never contains theme colors.
pub fn emphasize_word_spans(
    spans: Vec<ratatui::text::Span<'static>>,
    ranges: &[std::ops::Range<usize>],
    theme: &Theme,
    origin: LineOrigin,
) -> Vec<ratatui::text::Span<'static>> {
    use unicode_segmentation::UnicodeSegmentation;

    let accent = match origin {
        LineOrigin::Addition => theme.diff_add,
        LineOrigin::Deletion => theme.diff_del,
        LineOrigin::Context => return spans,
    };
    if ranges.is_empty() {
        return spans;
    }
    let mut result = Vec::with_capacity(spans.len() + ranges.len() * 2);
    let mut offset = 0;
    for span in spans {
        let text = span.content.as_ref();
        let end = offset + text.len();
        let mut cuts = vec![0, text.len()];
        // Ratatui segments each span independently. Cutting a grapheme in
        // two can discard combining marks or turn one ZWJ emoji into multiple
        // cells, so expand emphasis to whole graphemes within each syntax span.
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .collect();
        for range in ranges {
            if range.start > offset && range.start < end {
                let boundary = range.start - offset;
                let index = boundaries.partition_point(|&start| start <= boundary) - 1;
                cuts.push(boundaries[index]);
            }
            if range.end > offset && range.end < end {
                let boundary = range.end - offset;
                let index = boundaries.partition_point(|&start| start < boundary);
                cuts.push(boundaries[index]);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for cut in cuts.windows(2) {
            let changed = ranges
                .iter()
                .any(|range| range.start < offset + cut[1] && offset + cut[0] < range.end);
            let style = if changed {
                span.style.bg(word_emphasis_bg(
                    span.style.bg.unwrap_or(theme.panel_bg),
                    accent,
                ))
            } else {
                span.style
            };
            result.push(ratatui::text::Span::styled(
                text[cut[0]..cut[1]].to_owned(),
                style,
            ));
        }
        offset = end;
    }
    result
}

fn word_emphasis_bg(base: Color, accent: Color) -> Color {
    // Theme defaults and user RGB colors retain their dark/light character.
    // Convert ANSI colors as well, rather than using the foreground as a
    // background (which would make non-syntax text invisible).
    fn rgb(color: Color) -> (u8, u8, u8) {
        let index = match color {
            Color::Rgb(r, g, b) => return (r, g, b),
            Color::Indexed(index) => index,
            Color::Black | Color::Reset => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::Gray => 7,
            Color::DarkGray => 8,
            Color::LightRed => 9,
            Color::LightGreen => 10,
            Color::LightYellow => 11,
            Color::LightBlue => 12,
            Color::LightMagenta => 13,
            Color::LightCyan => 14,
            Color::White => 15,
        };
        if index < 16 {
            const ANSI: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (128, 0, 0),
                (0, 128, 0),
                (128, 128, 0),
                (0, 0, 128),
                (128, 0, 128),
                (0, 128, 128),
                (192, 192, 192),
                (128, 128, 128),
                (255, 0, 0),
                (0, 255, 0),
                (255, 255, 0),
                (0, 0, 255),
                (255, 0, 255),
                (0, 255, 255),
                (255, 255, 255),
            ];
            ANSI[index as usize]
        } else if index < 232 {
            let i = index - 16;
            let component = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (component(i / 36), component(i / 6 % 6), component(i % 6))
        } else {
            let v = 8 + (index - 232) * 10;
            (v, v, v)
        }
    }
    let (br, bg, bb) = rgb(base);
    let (ar, ag, ab) = rgb(accent);
    let mix = |b: u8, a: u8| ((u16::from(b) * 7 + u16::from(a) * 3) / 10) as u8;
    Color::Rgb(mix(br, ar), mix(bg, ag), mix(bb, ab))
}

pub fn expanded_context_style(theme: &Theme) -> Style {
    Style::default().fg(theme.expanded_context_fg)
}

pub fn diff_hunk_header_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg_dim)
        .bg(theme.section_highlight_bg())
}

pub fn file_header_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg_primary)
        .add_modifier(Modifier::BOLD)
}

pub fn reviewed_style(theme: &Theme) -> Style {
    Style::default().fg(theme.reviewed)
}

pub fn pending_style(theme: &Theme) -> Style {
    Style::default().fg(theme.pending)
}

pub fn border_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_unfocused)
    }
}

pub fn panel_style(theme: &Theme) -> Style {
    Style::default().bg(theme.panel_bg).fg(theme.fg_primary)
}

pub fn popup_style(theme: &Theme) -> Style {
    panel_style(theme)
}

pub fn status_bar_style(theme: &Theme) -> Style {
    Style::default()
        .bg(theme.status_bar_bg)
        .fg(theme.fg_primary)
}

pub fn mode_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.mode_fg)
        .bg(theme.mode_bg)
        .add_modifier(Modifier::BOLD)
}

pub fn file_status_style(theme: &Theme, status: char) -> Style {
    let color = match status {
        'A' => theme.file_added,
        'M' => theme.file_modified,
        'D' => theme.file_deleted,
        'R' => theme.file_renamed,
        _ => theme.fg_secondary,
    };
    Style::default().fg(color)
}

/// Override only the marker foreground, preserving the caller's legacy style.
pub fn selection_marker_style(theme: &Theme, fallback: Style) -> Style {
    match theme.selection_marker {
        Some(color) => fallback.fg(color),
        None => fallback,
    }
}

pub fn current_line_indicator_style(theme: &Theme) -> Style {
    selection_marker_style(theme, Style::default().fg(theme.border_focused))
}

pub fn hash_style(theme: &Theme) -> Style {
    Style::default().fg(theme.cursor_color)
}

pub fn branch_style(theme: &Theme) -> Style {
    Style::default().fg(theme.branch_name)
}

pub fn dir_icon_style(theme: &Theme) -> Style {
    Style::default().fg(theme.diff_hunk_header)
}

pub fn comment_type_style(_theme: &Theme, color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn comment_border_style(theme: &Theme, _color: Color) -> Style {
    // Match the file-header separator look so the comment box reads as a
    // structural divider rather than as colour-coded chrome. The comment-
    // type colour still lives on the [NOTE]/[ISSUE]/... label inside.
    file_header_style(theme)
}

/// Fixed palette used to tint comment chrome by author. Excludes red/green so
/// the colour never collides with diff add/del semantics. Cyan/yellow/magenta/
/// blue/light-magenta/light-cyan give us six visually distinct slots — enough
/// for a handful of agents alongside the human reviewer.
const AUTHOR_PALETTE: &[Color] = &[
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::LightMagenta,
    Color::LightCyan,
];

/// Deterministic palette colour for a given author name. Always returns
/// a colour; callers gate visibility separately via [`author_accent`].
/// Hashes via FNV-1a so the mapping is platform-independent and survives
/// across runs.
pub fn author_color_for(author: &str) -> Color {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in author.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    AUTHOR_PALETTE[(hash as usize) % AUTHOR_PALETTE.len()]
}

/// `Some(colour)` when `author` differs from the viewer (so the comment
/// chrome should advertise authorship), `None` when the comment is the
/// viewer's own.
pub fn author_accent(viewer: &str, author: &str) -> Option<Color> {
    if author == viewer {
        None
    } else {
        Some(author_color_for(author))
    }
}

/// Border style for a comment box, tinted by author when the comment is not
/// from the current viewer. Falls back to the neutral header style otherwise.
pub fn comment_border_style_for_author(theme: &Theme, viewer: &str, author: &str) -> Style {
    match author_accent(viewer, author) {
        Some(color) => Style::default().fg(color).add_modifier(Modifier::BOLD),
        None => file_header_style(theme),
    }
}

pub fn visual_selection_style(theme: &Theme) -> Style {
    Style::default().bg(theme.bg_highlight)
}

pub fn search_match_style(theme: &Theme) -> Style {
    Style::default().bg(theme.search_match_bg)
}

pub fn help_indicator_style(theme: &Theme) -> Style {
    Style::default().fg(theme.help_indicator).bg(theme.panel_bg)
}

pub fn range_bar_style(theme: &Theme) -> Style {
    current_line_indicator_style(theme)
}

pub fn error_inline_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.message_error_fg)
        .add_modifier(Modifier::BOLD)
}

pub fn pseudo_commit_tag_style(theme: &Theme) -> Style {
    Style::default().fg(theme.file_modified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_emphasis_splits_utf8_ranges_preserving_foregrounds_and_live_theme() {
        let theme = Theme::github_dark();
        let base = Style::default().fg(Color::Yellow).bg(theme.syntax_add_bg);
        let spans = vec![
            ratatui::text::Span::styled("aé", base),
            ratatui::text::Span::styled("猫z", base),
        ];
        let emphasized = emphasize_word_spans(spans.clone(), &[1..6], &theme, LineOrigin::Addition);
        assert_eq!(
            emphasized
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "aé猫z"
        );
        assert_eq!(
            emphasized
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>(),
            vec!["a", "é", "猫", "z"]
        );
        assert!(emphasized.iter().all(|s| s.style.fg == Some(Color::Yellow)));
        assert_eq!(emphasized[0].style, base);
        assert_ne!(emphasized[1].style.bg, base.bg);
        let light =
            emphasize_word_spans(spans, &[1..6], &Theme::github_light(), LineOrigin::Addition);
        assert_ne!(emphasized[1].style.bg, light[1].style.bg);
    }

    #[test]
    fn word_emphasis_preserves_combining_and_zwj_graphemes_when_rendered() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            text::{Line, Span},
            widgets::{Paragraph, Widget},
        };
        let theme = Theme::github_dark();
        let base = diff_add_style(&theme);
        for (text, changed) in [("e\u{301} tail", 1..3), ("👩‍💻 tail", 7..11)] {
            let source = vec![Span::styled(text, base)];
            let emphasized =
                emphasize_word_spans(source.clone(), &[changed], &theme, LineOrigin::Addition);
            let area = Rect::new(0, 0, 20, 1);
            let mut before = Buffer::empty(area);
            let mut after = Buffer::empty(area);
            Paragraph::new(Line::from(source)).render(area, &mut before);
            Paragraph::new(Line::from(emphasized)).render(area, &mut after);
            assert_eq!(
                before
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<Vec<_>>(),
                after
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<Vec<_>>(),
                "emphasis must preserve rendered graphemes and their width: {text:?}"
            );
            assert_ne!(before.content[0].bg, after.content[0].bg);
        }
    }

    #[test]
    fn word_emphasis_does_not_change_context_or_empty_ranges() {
        let theme = Theme::github_dark();
        let spans = vec![ratatui::text::Span::styled("hello", diff_del_style(&theme))];
        assert_eq!(
            emphasize_word_spans(spans.clone(), &[], &theme, LineOrigin::Deletion),
            spans
        );
        assert_eq!(
            emphasize_word_spans(spans.clone(), &[0..5], &theme, LineOrigin::Context),
            spans
        );
    }

    #[test]
    fn selection_marker_changes_only_marker_foregrounds() {
        let mut theme = Theme::github_dark();
        let legacy_pointer = selected_style(&theme);
        assert_eq!(
            selection_marker_style(&theme, legacy_pointer),
            legacy_pointer
        );
        assert_eq!(
            current_line_indicator_style(&theme).fg,
            Some(theme.border_focused)
        );
        assert_eq!(range_bar_style(&theme).fg, Some(theme.border_focused));
        let unchanged = (
            border_style(&theme, true),
            hash_style(&theme),
            selected_style(&theme),
            visual_selection_style(&theme),
        );

        theme.selection_marker = Some(Color::Rgb(36, 45, 56));
        assert_eq!(
            current_line_indicator_style(&theme).fg,
            theme.selection_marker
        );
        assert_eq!(range_bar_style(&theme).fg, theme.selection_marker);
        assert_eq!(
            selection_marker_style(&theme, legacy_pointer).bg,
            legacy_pointer.bg
        );
        assert_eq!(
            unchanged,
            (
                border_style(&theme, true),
                hash_style(&theme),
                selected_style(&theme),
                visual_selection_style(&theme)
            )
        );
    }
}
