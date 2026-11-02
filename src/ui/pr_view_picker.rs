//! Native PR-view picker, using the same themed popup/list pattern as the theme picker.
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::styles;

pub fn render_pr_view_picker(frame: &mut Frame, app: &mut App) {
    let Some(picker) = app.pr_view_picker.as_mut() else {
        return;
    };
    let theme = &app.theme;
    let area = frame.area();
    // Small for the common handful of views, bounded and scrollable for larger catalogs.
    let height = (picker.candidates.len().min(8) as u16 + 4).min(area.height);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(64.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" PR views ")
        .borders(Borders::ALL)
        .style(styles::popup_style(theme))
        .border_style(styles::border_style(theme, true));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [search, list_area, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    // Keep the end of long input visible, including multi-column Unicode.
    let query = format!("> {}", picker.query);
    let width = unicode_width::UnicodeWidthStr::width(query.as_str());
    let scroll = width
        .saturating_sub(search.width.saturating_sub(1) as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(Paragraph::new(query).scroll((0, scroll)), search);
    if search.width > 0 && search.height > 0 {
        frame.set_cursor_position((
            search.x + (width.saturating_sub(scroll as usize) as u16).min(search.width - 1),
            search.y,
        ));
    }
    let indices = picker.filtered_indices();
    if indices.is_empty() {
        frame.render_widget(
            Paragraph::new("No views match").style(styles::dim_style(theme)),
            list_area,
        );
    } else {
        let items = indices
            .iter()
            .map(|index| ListItem::new(picker.candidates[*index].name.clone()));
        frame.render_stateful_widget(
            List::new(items)
                .style(styles::panel_style(theme))
                .highlight_style(styles::selected_style(theme)),
            list_area,
            &mut picker.list_state,
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(
            "type to filter · ↑/↓ move · enter select · esc cancel",
        ))
        .style(styles::dim_style(theme)),
        footer,
    );
}
