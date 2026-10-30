use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::input::{
    Action,
    panel_bindings::{PanelBindings, leader_display},
};
use crate::ui::styles;

pub fn render_message_details(frame: &mut Frame, app: &mut App) {
    let Some(message) = app.message.as_ref() else {
        return;
    };
    let content = message.content.clone();
    let theme = &app.theme;
    let area = frame.area();

    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(styles::panel_style(theme))
        .border_style(styles::border_style(theme, true));
    let inner = block.inner(area);

    let paragraph = Paragraph::new(content.as_str())
        .style(
            Style::default()
                .fg(theme.message_error_fg)
                .bg(theme.panel_bg),
        )
        .wrap(Wrap { trim: false });
    app.help_state.total_lines = paragraph.line_count(inner.width);
    app.help_state.viewport_height = inner.height as usize;
    let max_offset = app
        .help_state
        .total_lines
        .saturating_sub(app.help_state.viewport_height);
    app.help_state.scroll_offset = app.help_state.scroll_offset.min(max_offset);
    let scroll_indicator = match (
        app.help_state.scroll_offset > 0,
        app.help_state.scroll_offset < max_offset,
    ) {
        (true, true) => " ↑↓",
        (true, false) => " ↑",
        (false, true) => " ↓",
        (false, false) => "",
    };
    frame.render_widget(
        block.title(format!(
            " Messages (j/k scroll, q/Esc close){scroll_indicator} "
        )),
        area,
    );
    frame.render_widget(
        paragraph.scroll((
            app.help_state.scroll_offset.min(u16::MAX as usize) as u16,
            0,
        )),
        inner,
    );
}

fn comment_mode_row(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(description.to_string()),
    ])
}

pub fn render_help(frame: &mut Frame, app: &mut App) {
    let theme = &app.theme;
    // Center over the diff pane (matches the submit-modal anchoring) so the
    // file list doesn't tug the popup's visual centre off to one side. Fall
    // back to the full frame when no diff area is laid out yet.
    let anchor = app.diff_area.unwrap_or(frame.area());
    let area = centered_rect(80, 90, anchor);

    // Clear the area behind the popup
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Help (j/k scroll, h/l pan, / search) - Press ? or Esc to close ")
        .borders(Borders::ALL)
        .style(styles::popup_style(theme))
        .border_style(styles::border_style(theme, true));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let help_text = help_lines(app.leader_key, app.comment_vim_enabled, &app.panel_bindings);

    // Update help state with total lines and viewport height
    let total_lines = help_text.len();
    let viewport_height = inner.height as usize;
    app.help_state.total_lines = total_lines;
    app.help_state.viewport_height = viewport_height;
    app.help_state.viewport_width = inner.width as usize;
    app.help_state.max_line_width = help_text.iter().map(Line::width).max().unwrap_or(0);
    let max_horizontal_offset = app.help_state.max_horizontal_offset();
    app.help_state.horizontal_offset = app.help_state.horizontal_offset.min(max_horizontal_offset);
    app.help_state.searchable_lines = help_text
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    // Calculate if we can scroll
    let can_scroll_up = app.help_state.scroll_offset > 0;
    let can_scroll_down = app.help_state.scroll_offset + viewport_height < total_lines;

    // Apply scroll offset
    let visible_lines: Vec<Line> = help_text
        .into_iter()
        .enumerate()
        .skip(app.help_state.scroll_offset)
        .take(viewport_height)
        .map(|(line_idx, line)| {
            if app.help_state.current_match_line == Some(line_idx) {
                line.style(styles::selected_style(theme))
            } else {
                line
            }
        })
        .collect();

    let paragraph = Paragraph::new(visible_lines)
        .style(styles::popup_style(theme))
        .scroll((
            0,
            app.help_state.horizontal_offset.min(u16::MAX as usize) as u16,
        ));
    frame.render_widget(paragraph, inner);

    // Render scroll indicators
    let indicator_style = styles::help_indicator_style(theme);

    if can_scroll_up {
        let up_indicator = Paragraph::new(Line::from(Span::styled("▲ more", indicator_style)));
        let up_area = Rect {
            x: inner.x + inner.width.saturating_sub(8),
            y: inner.y,
            width: 7,
            height: 1,
        };
        frame.render_widget(up_indicator, up_area);
    }

    if can_scroll_down {
        let down_indicator = Paragraph::new(Line::from(Span::styled("▼ more", indicator_style)));
        let down_area = Rect {
            x: inner.x + inner.width.saturating_sub(8),
            y: inner.y + inner.height.saturating_sub(1),
            width: 7,
            height: 1,
        };
        frame.render_widget(down_indicator, down_area);
    }
}

fn help_lines(
    leader: char,
    comment_vim_enabled: bool,
    bindings: &PanelBindings,
) -> Vec<Line<'static>> {
    let focus_left = bindings.labels(Action::FocusLeft, leader);
    let focus_right = bindings.labels(Action::FocusRight, leader);
    let focus_up = bindings.labels(Action::FocusUp, leader);
    let focus_down = bindings.labels(Action::FocusDown, leader);
    let toggle_file_list = bindings.labels(Action::ToggleFileList, leader);
    let toggle_single_file = bindings.labels(Action::ToggleSingleFileView, leader);
    let toggle_diff_view = bindings.labels(Action::ToggleDiffView, leader);
    let clear_include = bindings.labels(Action::FileTreeClearInclude, leader);
    let clear_exclude = bindings.labels(Action::FileTreeClearExclude, leader);
    let leader = leader_display(leader);
    let mut lines = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Scroll down/up"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl-e/y  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Scroll view down/up"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl-d/u  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Half page down/up"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl-f/b  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Full page down/up"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl-l    ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Redraw the whole screen (works in every mode)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  g/G       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Go to first/last file"),
        ]),
        Line::from(vec![
            Span::styled(
                "  {N}G      ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Go to source line N in current file"),
        ]),
        Line::from(vec![
            Span::styled(
                "  {/}       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to prev/next file"),
        ]),
        Line::from(vec![
            Span::styled(
                "  [/]       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to prev/next hunk"),
        ]),
        Line::from(vec![
            Span::styled(
                "  m/M       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to next/previous comment"),
        ]),
        Line::from(vec![
            Span::styled(
                "  /         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Search within diff (case-insensitive regex)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  n/N       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Next/prev search match (wraps)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Clear search highlighting"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Expand/collapse context (20 lines)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  S-Enter   ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Expand/collapse all hidden context"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Tab/S-Tab ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle focus next/previous panel"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {focus_left}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Move focus left (side-by-side: between the two sides)"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {focus_right}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Move focus right (side-by-side: between the two sides)"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {focus_up}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Move focus up between panes"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {focus_down}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Move focus down between panes"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {toggle_file_list}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle file list visibility"),
        ]),
        Line::from(vec![
            Span::styled(
                "  < / > / = ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Sidebar: narrow/widen (2%)/reset; or drag its diff boundary"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {leader}s        "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle commit selector visibility (also `:set commits!`)"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {toggle_single_file}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle current-file-only / continuous diff (also `:focus` / `:f`)"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {toggle_diff_view}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle unified/side-by-side diff view (also `:diff`)"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {leader}n        "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle relative line numbers (also `:set relativenumber!`)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  h/l       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Scroll diff left/right (or ←/→)"),
        ]),
        Line::from(vec![
            Span::styled("  h/← at 0 ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("Reveal + focus file list"),
        ]),
        Line::from(vec![
            Span::styled(
                "  l/→ in list",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Hide list + focus diff"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("Single-file view (`:focus`, `:f`, {toggle_single_file})"),
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(Span::raw(
            "  One file at a time. }/{ switch files, ]/[ walk across hunks. Default in `--all-files`.",
        )),
        Line::from(Span::raw(
            "  j/k stop at the file's edges. If the terminal reports key releases, release and press again to walk on.",
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Commit Selector (multi-commit reviews)",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(Span::raw(
            "  ✓ marks commits covered by your latest submitted review.",
        )),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate commits"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Space/Enter",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Toggle commit selection (updates diff)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  (/)       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cycle through individual commits"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Return focus to diff"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Review Target Selector",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Tab/S-Tab ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Switch tabs"),
        ]),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Move row"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Space     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle commit selection (Local tab only)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open selected target or load more"),
        ]),
        Line::from(vec![
            Span::styled(
                "  /         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Local filter for current tab"),
        ]),
        Line::from(vec![
            Span::styled(
                "  r         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle requested / all open PRs (replaces created-by-me scope)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  a         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("PRs: created by me / all open (GitHub); Sessions: show/hide empty"),
        ]),
        Line::from(vec![
            Span::styled(
                "  dd        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Delete saved review, with confirm (Sessions tab)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Return to the diff"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Submit Action Picker (PR mode)",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate review events"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Submit the selected event"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "File Tree",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Space     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle expand directory"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Expand dir / Jump to file"),
        ]),
        Line::from(vec![
            Span::styled(
                "  o         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Expand all directories"),
        ]),
        Line::from(vec![
            Span::styled(
                "  O         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Collapse all directories"),
        ]),
        Line::from(vec![
            Span::styled(
                "  i         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Filter to files matching a regex (hides others from tree + diff)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  e         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Filter out files matching a regex"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {clear_include}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Clear the include filter"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {clear_exclude}  "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Clear the exclude filter"),
        ]),
        Line::from(vec![
            Span::styled(
                "  /         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Search file paths; n/N step matches (selection only)"),
        ]),
        Line::from(Span::raw(
            "  Patterns are case-insensitive and match the whole relative path.",
        )),
        Line::from(Span::raw(
            "  :set noreviewed hides files already marked reviewed.",
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Comment Navigator",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate comments"),
        ]),
        Line::from(vec![
            Span::styled(
                "  h/l       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Scroll comment rows left/right"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to selected comment"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Review Actions",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  r         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle file reviewed"),
        ]),
        Line::from(vec![
            Span::styled(
                "  R         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle hunk reviewed"),
        ]),
        Line::from(vec![
            Span::styled(
                "  c         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Add line comment"),
        ]),
        Line::from(vec![
            Span::styled(
                "  C         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Add file comment"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {leader}c        "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Add review comment"),
        ]),
        Line::from(vec![
            Span::styled(
                "  i         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Edit comment at cursor"),
        ]),
        Line::from(vec![
            Span::styled(
                "  dd        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Delete comment at cursor"),
        ]),
        Line::from(vec![
            Span::styled(
                "  y         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Yank: mouse selection if any, else review to clipboard"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Y         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Copy comment at cursor to clipboard"),
        ]),
        Line::from(vec![
            Span::styled(
                "  e         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open focused file in editor config / $EDITOR"),
        ]),
        Line::from(vec![
            Span::styled(
                "  v/V       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Enter visual mode for range comments"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Visual Mode",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  j/k       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Extend selection down/up"),
        ]),
        Line::from(vec![
            Span::styled(
                "  c/Enter   ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Create comment for selected range"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc/v/V   ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel visual selection"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Comment Mode",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        comment_mode_row("Tab/S-Tab", "Cycle comment type next/previous"),
        comment_mode_row("Enter", "Save comment"),
        comment_mode_row("Ctrl-S", "Save comment"),
        comment_mode_row("Shift-Enter/Alt-Enter/Ctrl-J", "Insert newline"),
        comment_mode_row("Up/Down", "Previous/next displayed row (default editor)"),
        comment_mode_row("Ctrl-A/E", "Line start/end"),
        comment_mode_row("Ctrl/Alt-Left/Right", "Word left/right"),
        comment_mode_row("Cmd-Left/Right", "Line start/end (macOS)"),
        comment_mode_row("Esc/Ctrl-C", "Cancel"),
        comment_mode_row(
            "comment_vim",
            if comment_vim_enabled {
                "Vim ON (i/a:insert Esc:normal hjkl dd/ciw/x u; S-Enter:save S-Esc:discard :w/:q)"
            } else {
                "Set comment_vim=true (or :vim) for vim modal editing"
            },
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Commands",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  :{N}      ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to new-side line N in current file"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :o{N}     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Jump to old-side line N in current file (matches deletions)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Tab       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Complete or cycle command names in the : prompt"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :w        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Save review session"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :e        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                "Reload diff files and comments (in PR mode: refetch PR; may switch session)",
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  :edit     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open focused file in editor config / $EDITOR"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :clip     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Copy review to clipboard"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :copy-url ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Copy the open PR URL to clipboard (PR mode)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :wrap     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle line wrap in diff view"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :help     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open this help screen"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :messages ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open full details for the current error"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :summary  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Pending comments; j/k select, Enter jumps"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :stage    ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Stage reviewed files"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :diff     ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle unified/side-by-side diff view"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :theme    ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open the theme picker; :theme <name> applies directly"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :focus    ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "Toggle current-file-only / continuous diff (alias `:f`, {toggle_single_file})"
            )),
        ]),
        Line::from(vec![
            Span::styled(
                "  :targets  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open the review target selector"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :commits  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open the selector on Local (commits, staged/unstaged)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :prs      ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open the selector on Pull Requests"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :comments unresolved",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Show unresolved remote comments (PR mode, default)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :comments all",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Show all remote comments incl. resolved/outdated"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :comments hide",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Hide remote comments in PR mode"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :submit       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                "Pick a review event from a menu (Comment / Approve / Request changes / Draft)",
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  :submit comment",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Submit a COMMENT review (skips the picker, shows confirm modal)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :submit approve",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Submit an APPROVE review to the forge"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :submit request-changes",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Submit a REQUEST_CHANGES review to the forge"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :submit draft",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Push a pending (draft) review to the forge"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set relativenumber[!]",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Enable/toggle relative rendered-row numbers"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set norelativenumber",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Disable relative rendered-row numbers"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set commits",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Show inline commit selector"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set nocommits",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Hide inline commit selector"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set commits!",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Toggle inline commit selector"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :set reviewed",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Show files marked reviewed (noreviewed hides, reviewed! toggles)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :clear    ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Clear all comments and reviewed marks"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :clearc   ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Clear comments only"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :q        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Quit"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :wq       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Save and quit"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :version  ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Show tuicr version"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :update   ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Check for updates"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  /         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Search within this help (case-insensitive)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  n/N       ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Next/previous help search match"),
        ]),
        Line::from(vec![
            Span::styled(
                "  ?         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle this help"),
        ]),
    ];

    // Key/description rows have two spans; headers and prose have one.
    // Resolve the shared column after aliases and leader labels are expanded.
    let description_column = lines
        .iter()
        .filter(|line| line.spans.len() == 2)
        .map(|line| line.spans[0].content.trim_end().width())
        .max()
        .unwrap_or(0)
        + 1;
    for line in &mut lines {
        if let [key, description] = line.spans.as_mut_slice() {
            let label = key.content.trim_end();
            key.content =
                format!("{label}{}", " ".repeat(description_column - label.width())).into();
            description.content = description.content.trim_start().to_owned().into();
        }
    }
    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_toggle_help_shows_active_aliases_and_unbound_in_its_own_row() {
        for (values, label) in [("['<leader>v', 'F12']", "Space v / F12"), ("[]", "unbound")] {
            let config =
                toml::from_str(&format!("[keybindings]\ntoggle_diff_view={values}")).unwrap();
            let bindings = PanelBindings::from_config(&config).unwrap();
            let lines = help_lines(' ', false, &bindings);
            assert_description_columns_align(&lines);
            let row = lines
                .iter()
                .find(|line| {
                    line.spans.iter().any(|span| {
                        span.content == "Toggle unified/side-by-side diff view (also `:diff`)"
                    })
                })
                .unwrap();
            assert!(row.spans[0].content.contains(label));
            assert!(!row.spans[0].content.contains("Space d"));
        }
    }

    #[test]
    fn should_show_default_and_fixed_bindings_with_the_configured_leader() {
        let text = help_lines(',', false, &PanelBindings::default())
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<String>();
        for key in [",h", ",l", ",k", ",j", ",e", ",f", ",d", ",s", ",n", ",c"] {
            assert!(text.contains(key), "missing {key}");
        }
        assert!(!text.contains(";f"));
    }

    #[test]
    fn should_show_remapped_aliases_and_unbound_actions_everywhere_in_help() {
        let config: crate::config::AppConfig = toml::from_str(
            r#"
            leader = ","
            [keybindings]
            focus_left = ["Alt-Left", "<leader>a"]
            toggle_file_list = []
            toggle_single_file_view = ["<leader>v"]
            file_tree_clear_include = []
            file_tree_clear_exclude = ["<leader>E", "Alt-E"]
        "#,
        )
        .unwrap();
        let bindings = PanelBindings::from_config(&config).unwrap();
        let text = help_lines(',', false, &bindings)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Alt-Left / ,a"));
        assert!(
            text.lines().any(|line| {
                line.contains("unbound") && line.contains("Clear the include filter")
            })
        );
        assert!(text.lines().any(|line| {
            line.contains(",E / Alt-E") && line.contains("Clear the exclude filter")
        }));
        assert!(
            text.lines()
                .any(|line| line.contains("unbound") && line.contains("Toggle file list"))
        );
        assert_eq!(
            text.matches(",v").count(),
            3,
            "toggle row, section heading, and :focus description"
        );
        assert!(!text.contains(",f"));
        assert!(!text.contains(",h"));
        assert!(!text.contains(",e"));
    }

    #[test]
    fn should_show_each_filter_clear_on_its_own_help_row() {
        for (config, include, exclude) in [
            ("", "I", "E"),
            (
                "[keybindings]\nfile_tree_clear_include=['Alt-i']\nfile_tree_clear_exclude=['Alt-e']",
                "Alt-i",
                "Alt-e",
            ),
            (
                "leader=' '\n[keybindings]\nfile_tree_clear_include=['<leader>I', 'Alt-i']\nfile_tree_clear_exclude=['<leader>E', 'Alt-e']",
                "Space I / Alt-i",
                "Space E / Alt-e",
            ),
            (
                "[keybindings]\nfile_tree_clear_include=[]\nfile_tree_clear_exclude=[]",
                "unbound",
                "unbound",
            ),
        ] {
            let config: crate::config::AppConfig = toml::from_str(config).unwrap();
            let bindings = PanelBindings::from_config(&config).unwrap();
            let lines = help_lines(config.leader.unwrap_or(';'), false, &bindings);
            let rows: Vec<_> = lines
                .iter()
                .filter(|line| {
                    line.spans.iter().any(|span| {
                        span.content == "Clear the include filter"
                            || span.content == "Clear the exclude filter"
                    })
                })
                .collect();
            assert_eq!(rows.len(), 2, "each clear action needs its own row");
            for (row, key, description) in [
                (rows[0], include, "Clear the include filter"),
                (rows[1], exclude, "Clear the exclude filter"),
            ] {
                assert_eq!(row.spans.len(), 2);
                assert_eq!(row.spans[0].content.trim(), key);
                assert!(row.spans[0].style.add_modifier.contains(Modifier::BOLD));
                assert_eq!(row.spans[1].content, description);
            }
        }
    }

    #[test]
    fn should_explain_single_file_toggle_as_current_only_versus_continuous() {
        let text = help_lines(';', false, &PanelBindings::default())
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<String>();
        assert!(text.contains("current-file-only / continuous diff"));
    }

    #[test]
    fn should_show_space_leader_consistently_for_default_and_configured_help() {
        for overrides in ["", "\n[keybindings]\nfocus_left=['<leader>a']"] {
            let config: crate::config::AppConfig =
                toml::from_str(&format!("leader=' '{overrides}")).unwrap();
            let bindings = PanelBindings::from_config(&config).unwrap();
            let text = help_lines(config.leader.unwrap(), false, &bindings)
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
                .collect::<String>();
            let left = if overrides.is_empty() { "h" } else { "a" };
            for suffix in [left, "l", "k", "j", "e", "f", "d", "s", "n", "c"] {
                assert!(
                    text.contains(&format!("Space {suffix}")),
                    "missing Space {suffix}"
                );
            }
            assert!(!text.contains('\u{2423}'));
        }
    }

    #[test]
    fn should_align_space_leader_and_configured_filter_clear_aliases() {
        let config: crate::config::AppConfig = toml::from_str(
            r#"
            leader = " "
            [keybindings]
            file_tree_clear_include = ["<leader>I", "Alt-I"]
            file_tree_clear_exclude = []
            "#,
        )
        .unwrap();
        let bindings = PanelBindings::from_config(&config).unwrap();
        let lines = help_lines(' ', false, &bindings);
        assert_description_columns_align(&lines);
        for (description, key) in [
            ("Clear the include filter", "Space I / Alt-I"),
            ("Clear the exclude filter", "unbound"),
        ] {
            let clear_row = lines
                .iter()
                .find(|line| line.spans.len() == 2 && line.spans[1].content == description)
                .unwrap();
            assert_eq!(clear_row.spans[0].content.trim(), key);
        }
        for key in ["Space h", "Space s", "Space n", "Space c"] {
            assert!(
                lines.iter().any(|line| line
                    .spans
                    .first()
                    .is_some_and(|span| span.content.trim() == key)),
                "missing {key}"
            );
        }
    }

    fn assert_description_columns_align(lines: &[Line<'_>]) {
        let rows: Vec<_> = lines.iter().filter(|line| line.spans.len() == 2).collect();
        let longest_key = rows
            .iter()
            .map(|line| line.spans[0].content.trim_end().width())
            .max()
            .unwrap();
        for row in rows {
            assert_eq!(row.spans[0].width(), longest_key + 1, "{row:?}");
            assert!(!row.spans[1].content.starts_with(' '), "{row:?}");
            assert_eq!(
                row.spans[0].style,
                Style::default().add_modifier(Modifier::BOLD)
            );
            assert_eq!(row.spans[1].style, Style::default());
        }
    }

    #[test]
    fn should_align_all_help_descriptions_one_space_after_the_longest_key() {
        for vim_enabled in [false, true] {
            let lines = help_lines(';', vim_enabled, &PanelBindings::default());
            assert_description_columns_align(&lines);
            assert_eq!(
                lines[0],
                Line::from(Span::styled(
                    "Navigation",
                    Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ))
            );
            assert_eq!(lines[1], Line::from(""));
            assert!(lines.contains(&Line::from(Span::raw(
                "  One file at a time. }/{ switch files, ]/[ walk across hunks. Default in `--all-files`.",
            ))));
        }
    }

    #[test]
    fn should_align_configured_aliases_and_unbound_using_display_width() {
        let config: crate::config::AppConfig = toml::from_str(
            r#"
            leader = "界"
            [keybindings]
            focus_left = ["Alt-Left", "Ctrl-Left", "<leader>界", "<leader>a"]
            toggle_file_list = []
        "#,
        )
        .unwrap();
        let bindings = PanelBindings::from_config(&config).unwrap();
        let lines = help_lines('界', false, &bindings);
        assert_description_columns_align(&lines);
        assert!(
            lines
                .iter()
                .any(|line| line.spans.len() == 2 && line.spans[0].content.trim() == "unbound")
        );
        let aliases = lines
            .iter()
            .find(|line| {
                line.spans.len() == 2 && line.spans[1].content.starts_with("Move focus left")
            })
            .unwrap();
        assert_eq!(
            aliases.spans[0].content.trim(),
            "Alt-Left / Ctrl-Left / 界界 / 界a"
        );
        assert_ne!(aliases.spans[0].content.len(), aliases.spans[0].width());
    }
}
