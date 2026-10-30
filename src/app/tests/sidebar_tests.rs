use super::render_perf_tests::{app_with, file};
use crate::app::*;
use crate::handler::{handle_diff_action, handle_mouse_event};
use crate::input::keybindings::map_key_to_action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

fn draw(app: &mut App, width: u16, height: u16) {
    Terminal::new(TestBackend::new(width, height))
        .unwrap()
        .draw(|frame| crate::ui::render(frame, app))
        .unwrap();
}

fn key(app: &mut App, ch: char) {
    let action = map_key_to_action(
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
        app.input_mode,
        app.leader_key,
    );
    handle_diff_action(app, action);
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    handle_mouse_event(
        app,
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    );
}

#[test]
fn sidebar_keyboard_resizes_and_resets_outer_panel() {
    let mut app = app_with(vec![file("example.rs", 60)]);
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 20);
    key(&mut app, '>');
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 22);
    assert_eq!(app.diff_area.unwrap().width, 78);
    key(&mut app, '<');
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 20);
    key(&mut app, '>');
    key(&mut app, '=');
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 20);
}

#[test]
fn sidebar_boundary_drag_is_not_a_text_selection() {
    let mut app = app_with(vec![file("example.rs", 60)]);
    draw(&mut app, 100, 30);
    let sidebar = app.file_list_area.unwrap();
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        sidebar.right() - 1,
        sidebar.y + 2,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        39,
        sidebar.y + 2,
    );
    draw(&mut app, 100, 30);
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        39,
        sidebar.y + 2,
    );
    assert_eq!(app.file_list_area.unwrap().width, 40);
    assert!(app.visual_selection.is_none());
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn sidebar_resize_preserves_cursor_and_scroll_after_wrapped_comment() {
    for mode in [DiffViewMode::Unified, DiffViewMode::SideBySide] {
        let mut app = app_with(vec![file("example.rs", 60)]);
        app.diff_view_mode = mode;
        app.diff_state.wrap_lines = true;
        app.session.review_comments.push(Comment::new(
            "long review comment ".repeat(30),
            CommentType::None,
            None,
        ));
        app.rebuild_annotations();
        draw(&mut app, 100, 30);
        app.go_to_source_line(30, LineSide::New);
        let line = app.get_line_at_cursor();
        let top = format!("{:?}", app.line_annotations[app.diff_state.scroll_offset]);
        for _ in 0..10 {
            key(&mut app, '>');
        }
        draw(&mut app, 100, 30);
        assert_eq!(app.get_line_at_cursor(), line);
        assert_eq!(
            format!("{:?}", app.line_annotations[app.diff_state.scroll_offset]),
            top
        );
    }
}

#[test]
fn sidebar_width_survives_visibility_view_and_annotation_changes() {
    let mut app = app_with(vec![file("example.rs", 60)]);
    app.default_sidebar_width = 35;
    key(&mut app, '=');
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 35);
    key(&mut app, '>');
    app.toggle_file_list();
    draw(&mut app, 100, 30);
    assert!(app.sidebar_area.is_none());
    app.toggle_file_list();
    app.toggle_diff_view_mode();
    app.diff_state.wrap_lines = true;
    app.rebuild_annotations();
    draw(&mut app, 100, 30);
    assert_eq!(app.file_list_area.unwrap().width, 37);
    assert!(!app.dirty);
}

#[test]
fn sidebar_clamps_tiny_terminals_without_losing_preference() {
    let mut app = app_with(vec![file("example.rs", 3)]);
    for preference in [1, 20, 99] {
        app.sidebar_width = preference;
        for width in [0, 1, 2, 5, 10, 30, 80, 100, 200] {
            draw(&mut app, width, 20);
            let sidebar = app.sidebar_area.unwrap();
            let diff = app.diff_area.unwrap();
            assert_eq!(sidebar.width + diff.width, width);
            assert_eq!(app.sidebar_width, preference);
            if width >= 32 {
                assert!(sidebar.width >= 12);
                assert!(diff.width >= 20);
            }
        }
    }
}

#[test]
fn sidebar_drag_includes_comment_navigator_and_consumes_release() {
    let mut app = app_with(vec![file("example.rs", 60)]);
    app.session
        .review_comments
        .push(Comment::new("A comment".into(), CommentType::None, None));
    app.rebuild_annotations();
    draw(&mut app, 100, 30);
    let comments = app.comment_navigator_area.unwrap();
    let cursor = app.diff_state.cursor_line;
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        comments.right() - 1,
        comments.y + 1,
    );
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 49, 0);
    draw(&mut app, 100, 30);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 70, 12);
    assert_eq!(app.file_list_area.unwrap().width, 50);
    assert_eq!(app.comment_navigator_area.unwrap().width, 50);
    assert_eq!(app.diff_state.cursor_line, cursor);
    assert!(!app.sidebar_resize_active);
    assert!(app.visual_selection.is_none());
}

#[test]
fn sidebar_mouse_ignores_prompts_and_normal_diff_drag_still_selects() {
    let mut app = app_with(vec![file("example.rs", 60)]);
    draw(&mut app, 100, 30);
    let sidebar = app.sidebar_area.unwrap();
    app.begin_file_tree_prompt(FileTreePrompt::Include);
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        sidebar.right() - 1,
        sidebar.y + 2,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        49,
        sidebar.y + 2,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        49,
        sidebar.y + 2,
    );
    assert_eq!(app.sidebar_width, 20);
    app.file_filter.draft = None;
    app.go_to_source_line(10, LineSide::New);
    draw(&mut app, 100, 30);
    let diff = app.diff_inner_area.unwrap();
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        diff.x + 10,
        diff.y + 5,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        diff.x + 20,
        diff.y + 6,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        diff.x + 20,
        diff.y + 6,
    );
    assert_eq!(app.input_mode, InputMode::VisualSelect);
    assert!(app.visual_selection.is_some());
    assert_eq!(app.sidebar_width, 20);
}

#[test]
fn sidebar_keys_respect_modifiers_leaders_and_text_entry() {
    use crate::input::{Action, map_file_tree_mode_with_q_quits, map_file_tree_prompt_mode};
    for (ch, expected) in [
        ('<', Action::NarrowSidebar),
        ('>', Action::WidenSidebar),
        ('=', Action::ResetSidebarWidth),
    ] {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            let key = KeyEvent::new(KeyCode::Char(ch), modifiers);
            assert_eq!(map_key_to_action(key, InputMode::Normal, ';'), expected);
            assert_eq!(
                map_key_to_action(key, InputMode::Normal, ch),
                Action::PendingLeaderCommand
            );
            assert_eq!(
                map_file_tree_mode_with_q_quits(key, ch, false),
                Action::PendingLeaderCommand
            );
            assert_eq!(map_file_tree_prompt_mode(key), Action::InsertChar(ch));
            for mode in [InputMode::Comment, InputMode::Command, InputMode::Search] {
                assert_eq!(map_key_to_action(key, mode, ';'), Action::InsertChar(ch));
            }
        }
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            assert_eq!(
                map_key_to_action(
                    KeyEvent::new(KeyCode::Char(ch), modifiers),
                    InputMode::Normal,
                    ';'
                ),
                Action::None
            );
        }
    }
}

#[test]
fn sidebar_reverses_immediately_after_dragging_or_keying_past_limits() {
    for (column, reverse) in [(u16::MAX, '<'), (0, '>')] {
        let mut app = app_with(vec![file("example.rs", 60)]);
        draw(&mut app, 100, 30);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 19, 5);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), column, 5);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), column, 5);
        draw(&mut app, 100, 30);
        let limit = app.sidebar_area.unwrap().width;
        key(&mut app, reverse);
        draw(&mut app, 100, 30);
        assert_ne!(
            app.sidebar_area.unwrap().width,
            limit,
            "reverse after drag {column}"
        );
        let outward = if reverse == '<' { '>' } else { '<' };
        for _ in 0..100 {
            key(&mut app, outward);
        }
        draw(&mut app, 100, 30);
        let limit = app.sidebar_area.unwrap().width;
        key(&mut app, reverse);
        draw(&mut app, 100, 30);
        assert_ne!(
            app.sidebar_area.unwrap().width,
            limit,
            "reverse after key {outward}"
        );
    }
}
