use super::render_perf_tests::{app_with, file};
use crate::app::*;
use crate::handler::{
    handle_comment_navigator_action, handle_commit_selector_action, handle_diff_action,
    handle_file_list_action,
};
use crate::input::Action;

#[test]
fn panel_actions_work_from_every_normal_review_pane() {
    let handlers: [fn(&mut App, Action); 4] = [
        handle_diff_action,
        handle_file_list_action,
        handle_commit_selector_action,
        handle_comment_navigator_action,
    ];
    for (handle, panel) in handlers.into_iter().zip([
        FocusedPanel::Diff,
        FocusedPanel::FileList,
        FocusedPanel::CommitSelector,
        FocusedPanel::Comments,
    ]) {
        let mut app = app_with(vec![file("one.rs", 10), file("two.rs", 10)]);
        app.focused_panel = panel;
        app.leader_key = ' ';
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(
            crate::handler::map_panel_key(&app, key, true),
            Some(Action::ToggleDiffView)
        );
        app.diff_view_mode = DiffViewMode::Unified;
        handle(&mut app, Action::ToggleDiffView);
        assert_eq!(app.diff_view_mode, DiffViewMode::SideBySide);
        handle(&mut app, Action::ToggleDiffView);
        assert_eq!(app.diff_view_mode, DiffViewMode::Unified);
        app.show_file_list = true;
        handle(&mut app, Action::ToggleFileList);
        assert!(!app.show_file_list);
        handle(&mut app, Action::ToggleFileList);
        assert!(app.show_file_list);
        assert!(!app.is_single_file_view);
        handle(&mut app, Action::ToggleSingleFileView);
        assert!(app.is_single_file_view);
        handle(&mut app, Action::ToggleSingleFileView);
        assert!(!app.is_single_file_view);
    }
}

#[test]
fn panel_focus_actions_preserve_horizontal_layout_and_hidden_sidebar_semantics() {
    for layout in [DiffViewMode::Unified, DiffViewMode::SideBySide] {
        let mut app = app_with(vec![file("one.rs", 10)]);
        app.diff_view_mode = layout;
        app.show_file_list = true;
        app.focused_panel = FocusedPanel::Diff;
        app.cursor_side = LineSide::New;
        handle_diff_action(&mut app, Action::FocusLeft);
        if layout == DiffViewMode::SideBySide {
            assert_eq!(app.focused_panel, FocusedPanel::Diff);
            assert_eq!(app.cursor_side, LineSide::Old);
            handle_diff_action(&mut app, Action::FocusLeft);
        }
        assert_eq!(app.focused_panel, FocusedPanel::FileList);
        handle_file_list_action(&mut app, Action::FocusRight);
        assert_eq!(app.focused_panel, FocusedPanel::Diff);
        handle_diff_action(&mut app, Action::FocusRight);
        assert_eq!(app.cursor_side, LineSide::New);
        app.show_file_list = false;
        handle_diff_action(&mut app, Action::FocusLeft);
        handle_diff_action(&mut app, Action::FocusLeft);
        assert_eq!(app.focused_panel, FocusedPanel::Diff);
    }
}

#[test]
fn panel_focus_vertical_preserves_visible_pane_stack() {
    let mut app = app_with(vec![file("one.rs", 10)]);
    app.focused_panel = FocusedPanel::Diff;
    handle_diff_action(&mut app, Action::FocusUp);
    assert_eq!(app.focused_panel, FocusedPanel::Diff); // No commit pane.
    app.review_commits = vec![App::staged_commit_entry(), App::unstaged_commit_entry()];
    app.diff_source = DiffSource::StagedAndUnstaged;
    app.show_commit_selector = true;
    handle_diff_action(&mut app, Action::FocusUp);
    assert_eq!(app.focused_panel, FocusedPanel::CommitSelector);
    handle_commit_selector_action(&mut app, Action::FocusDown);
    assert_eq!(app.focused_panel, FocusedPanel::Diff);
    app.show_commit_selector = false;
    handle_diff_action(&mut app, Action::FocusUp);
    assert_eq!(app.focused_panel, FocusedPanel::Diff);
    app.session
        .review_comments
        .push(Comment::new("Review note".into(), CommentType::None, None));
    app.rebuild_annotations();
    app.focused_panel = FocusedPanel::FileList;
    handle_file_list_action(&mut app, Action::FocusDown);
    assert_eq!(app.focused_panel, FocusedPanel::Comments);
    handle_comment_navigator_action(&mut app, Action::FocusUp);
    assert_eq!(app.focused_panel, FocusedPanel::FileList);
    app.session.review_comments.clear();
    app.rebuild_annotations();
    handle_file_list_action(&mut app, Action::FocusDown);
    assert_eq!(app.focused_panel, FocusedPanel::Diff);
}

#[test]
fn panel_bindings_are_scoped_out_of_every_other_mode_and_tree_text_drafts() {
    use crate::handler::map_panel_key;
    use crate::input::panel_bindings::PanelBindings;
    use crate::input::{map_file_tree_prompt_mode, map_key_to_action};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let cfg = toml::from_str("[keybindings]\ntoggle_diff_view = ['x', '<leader>a']").unwrap();
    let mut app = app_with(vec![file("one.rs", 10)]);
    app.panel_bindings = PanelBindings::from_config(&cfg).unwrap();
    let direct = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let prefixed = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(
        map_panel_key(&app, direct, false),
        Some(Action::ToggleDiffView)
    );
    assert_eq!(
        map_panel_key(&app, prefixed, true),
        Some(Action::ToggleDiffView)
    );
    for mode in [
        InputMode::Comment,
        InputMode::Command,
        InputMode::Search,
        InputMode::Help,
        InputMode::MessageDetails,
        InputMode::Summary,
        InputMode::Confirm,
        InputMode::CommitSelect,
        InputMode::VisualSelect,
        InputMode::SubmitResolver,
        InputMode::SubmitConfirm,
        InputMode::SubmitActionPicker,
        InputMode::ThemePicker,
    ] {
        app.input_mode = mode;
        assert_eq!(map_panel_key(&app, direct, false), None, "{mode:?}");
        assert_eq!(map_panel_key(&app, prefixed, true), None, "{mode:?}");
        if matches!(
            mode,
            InputMode::Comment | InputMode::Command | InputMode::Search
        ) {
            assert_eq!(
                map_key_to_action(direct, mode, app.leader_key),
                Action::InsertChar('x')
            );
        }
    }
    app.input_mode = InputMode::Normal;
    for prompt in [
        FileTreePrompt::Include,
        FileTreePrompt::Exclude,
        FileTreePrompt::Search,
    ] {
        app.begin_file_tree_prompt(prompt);
        assert_eq!(map_panel_key(&app, direct, false), None);
        assert_eq!(map_panel_key(&app, prefixed, true), None);
        assert_eq!(map_file_tree_prompt_mode(direct), Action::InsertChar('x'));
        app.cancel_file_tree_prompt();
    }
}

#[test]
fn filter_clear_bindings_only_resolve_in_tree_navigation() {
    use crate::handler::map_panel_key;
    use crate::input::PanelBindings;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = app_with(vec![file("one.rs", 10)]);
    let cfg = toml::from_str("[keybindings]\nfile_tree_clear_include=['x', '<leader>a']\nfile_tree_clear_exclude=['<leader>E']\ntoggle_file_list=['E']").unwrap();
    app.panel_bindings = PanelBindings::from_config(&cfg).unwrap();
    for panel in [
        FocusedPanel::Diff,
        FocusedPanel::FileList,
        FocusedPanel::Comments,
        FocusedPanel::CommitSelector,
    ] {
        app.focused_panel = panel;
        for (c, leader) in [('x', false), ('a', true)] {
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            assert_eq!(
                map_panel_key(&app, key, leader),
                if panel == FocusedPanel::FileList {
                    Some(Action::FileTreeClearInclude)
                } else {
                    None
                }
            );
        }
        assert_eq!(
            map_panel_key(
                &app,
                KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT),
                false
            ),
            Some(Action::ToggleFileList)
        );
    }
    app.focused_panel = FocusedPanel::FileList;
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    for mode in [
        InputMode::Comment,
        InputMode::Command,
        InputMode::Search,
        InputMode::Help,
        InputMode::MessageDetails,
        InputMode::Summary,
        InputMode::Confirm,
        InputMode::CommitSelect,
        InputMode::VisualSelect,
        InputMode::SubmitResolver,
        InputMode::SubmitConfirm,
        InputMode::SubmitActionPicker,
        InputMode::ThemePicker,
    ] {
        app.input_mode = mode;
        assert_eq!(map_panel_key(&app, key, true), None);
    }
    app.input_mode = InputMode::Normal;
    for prompt in [
        FileTreePrompt::Include,
        FileTreePrompt::Exclude,
        FileTreePrompt::Search,
    ] {
        app.begin_file_tree_prompt(prompt);
        assert_eq!(map_panel_key(&app, key, true), None);
        app.cancel_file_tree_prompt();
    }
}

#[test]
fn filter_clear_defaults_do_not_steal_the_configured_leader_prefix() {
    use crate::handler::map_panel_key;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = app_with(vec![file("one.rs", 10)]);
    app.focused_panel = FocusedPanel::FileList;
    for leader in ['I', 'E'] {
        app.leader_key = leader;
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            let key = KeyEvent::new(KeyCode::Char(leader), modifiers);
            assert_eq!(map_panel_key(&app, key, false), None);
        }
    }
}

#[test]
fn diff_toggle_shortcut_matches_command_and_pristine_guard() {
    for pristine in [false, true] {
        for initial in [DiffViewMode::Unified, DiffViewMode::SideBySide] {
            let mut shortcut = app_with(vec![file("one.rs", 10)]);
            let mut command = app_with(vec![file("one.rs", 10)]);
            for app in [&mut shortcut, &mut command] {
                app.is_pristine_mode = pristine;
                app.diff_view_mode = initial;
                app.rebuild_annotations();
                app.colored_hunks.insert((0, 0));
            }
            handle_diff_action(&mut shortcut, Action::ToggleDiffView);
            command.enter_command_mode();
            command.command_buffer.push_str("diff");
            crate::handler::handle_command_action(&mut command, Action::SubmitInput);
            assert_eq!(shortcut.diff_view_mode, command.diff_view_mode);
            assert_eq!(
                format!("{:?}", shortcut.line_annotations),
                format!("{:?}", command.line_annotations)
            );
            assert_eq!(
                shortcut.message.as_ref().map(|m| &m.content),
                command.message.as_ref().map(|m| &m.content)
            );
            assert_eq!(shortcut.colored_hunks, command.colored_hunks);
            assert_eq!(shortcut.colored_hunks.is_empty(), !pristine);
            if pristine {
                assert_eq!(shortcut.diff_view_mode, initial);
            }
        }
    }
}

#[test]
fn open_pr_browser_dispatches_from_every_review_pane_but_not_other_modes() {
    use crate::handler::map_panel_key;
    use crate::input::PanelBindings;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let config =
        toml::from_str("leader=' '\n[keybindings]\nopen_pr_in_browser=['F12', '<leader>b']")
            .unwrap();
    let mut app = app_with(vec![file("one.rs", 10)]);
    app.panel_bindings = PanelBindings::from_config(&config).unwrap();
    app.leader_key = ' ';
    let keys = [
        (KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE), false),
        (KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE), true),
    ];
    let handlers: [fn(&mut App, Action); 4] = [
        handle_diff_action,
        handle_file_list_action,
        handle_commit_selector_action,
        handle_comment_navigator_action,
    ];
    for (handler, panel) in handlers.into_iter().zip([
        FocusedPanel::Diff,
        FocusedPanel::FileList,
        FocusedPanel::CommitSelector,
        FocusedPanel::Comments,
    ]) {
        app.focused_panel = panel;
        for (key, leader) in keys {
            let action = map_panel_key(&app, key, leader).unwrap();
            assert_eq!(action, Action::OpenPrInBrowser);
            handler(&mut app, action);
            assert!(
                app.message
                    .as_ref()
                    .unwrap()
                    .content
                    .contains("only applies in PR mode")
            );
        }
    }
    for mode in [
        InputMode::Comment,
        InputMode::Command,
        InputMode::Search,
        InputMode::Help,
        InputMode::MessageDetails,
        InputMode::Summary,
        InputMode::Confirm,
        InputMode::CommitSelect,
        InputMode::VisualSelect,
        InputMode::SubmitResolver,
        InputMode::SubmitConfirm,
        InputMode::SubmitActionPicker,
        InputMode::ThemePicker,
    ] {
        app.input_mode = mode;
        for (key, leader) in keys {
            assert_eq!(map_panel_key(&app, key, leader), None, "{mode:?}");
        }
    }
    app.input_mode = InputMode::Normal;
    app.focused_panel = FocusedPanel::FileList;
    for prompt in [
        FileTreePrompt::Include,
        FileTreePrompt::Exclude,
        FileTreePrompt::Search,
    ] {
        app.begin_file_tree_prompt(prompt);
        for (key, leader) in keys {
            assert_eq!(map_panel_key(&app, key, leader), None);
        }
        app.cancel_file_tree_prompt();
    }
}
