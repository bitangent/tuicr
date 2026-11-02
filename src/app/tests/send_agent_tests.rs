use super::render_perf_tests::{app_with, file};
use crate::app::*;
use crate::forge::remote_comments::{RemoteCommentSide, RemoteReviewComment, RemoteReviewThread};
use crate::forge::traits::{ForgeRepository, PrSessionKey};
use crate::handler::{handle_command_action, handle_diff_action, map_panel_key};
use crate::input::{Action, PanelBindings};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn review(queue: &Path) -> App {
    let mut app = app_with(vec![file("src/main.rs", 10)]);
    app.agent_queue_dir = Some(queue.to_path_buf());
    app.session.add_diff_file(&app.diff_files[0]);
    app.session
        .get_file_mut(&PathBuf::from("src/main.rs"))
        .unwrap()
        .add_line_comment(
            2,
            Comment::new_with_range(
                "Fix café\n\n```rust\nx();\n```".into(),
                CommentType::from_id("issue"),
                Some(LineSide::Old),
                crate::model::LineRange::new(2, 4),
            )
            .with_author("Reviewer")
            .with_commit_id("abc1234deadbeef"),
        );
    app.comment_types = vec![CommentTypeDefinition {
        id: "issue".into(),
        label: "blocking".into(),
        definition: Some("fix before merge".into()),
        color: None,
    }];
    app.session.session_notes = Some("Batch summary".into());
    app.export.intro = Some("Please address these exact comments.".into());
    app.dirty = true;
    app
}

fn batches(queue: &Path) -> Vec<serde_json::Value> {
    std::fs::read_dir(queue.join("pending"))
        .unwrap()
        .map(|entry| {
            serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap()
        })
        .collect()
}

fn command(app: &mut App, text: &str) {
    app.enter_command_mode();
    app.command_buffer = text.into();
    handle_command_action(app, Action::SubmitInput);
}

#[test]
fn send_agent_matches_y_export_exactly_for_commit_and_pr_context() {
    for pr_mode in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let queue = temp.path().join("queue");
        let mut app = review(&queue);
        if pr_mode {
            let key = PrSessionKey::new(
                ForgeRepository::github("github.com", "owner", "repo"),
                42,
                "abcdef0123456",
            );
            app.session.pr_session_key = Some(key.clone());
            app.session.diff_source = SessionDiffSource::PullRequest;
            app.diff_source = DiffSource::PullRequest(Box::new(PullRequestDiffSource {
                key,
                base_sha: "base12345678".into(),
                title: "Review title".into(),
                url: "https://github.com/owner/repo/pull/42".into(),
                head_ref_name: "feature".into(),
                base_ref_name: "main".into(),
                state: "OPEN".into(),
                closed: false,
                merged: false,
            }));
            app.forge_review_threads = vec![RemoteReviewThread {
                id: "thread".into(),
                path: "src/main.rs".into(),
                line: Some(7),
                side: RemoteCommentSide::Right,
                is_resolved: false,
                is_outdated: false,
                comments: vec![RemoteReviewComment {
                    id: "remote-comment".into(),
                    author: Some("alice".into()),
                    body: "Unresolved upstream feedback".into(),
                    created_at: None,
                    in_reply_to: None,
                    url: "https://github.com/owner/repo/pull/42#discussion_1".into(),
                }],
            }];
        } else {
            app.diff_source =
                DiffSource::CommitRange(vec!["abc1234deadbeef".into(), "def5678abcdef".into()]);
        }
        // y's stdout mode exposes its exact clipboard-builder bytes without needing a desktop clipboard.
        app.output_to_stdout = true;
        handle_diff_action(&mut app, Action::ExportToClipboard);
        let copied = app.pending_stdout_output.take().unwrap();
        assert!(!queue.exists(), "y must not enqueue");
        app.should_quit = false;
        let before = serde_json::to_value(&app.session).unwrap();
        handle_diff_action(&mut app, Action::SendToAgent);
        assert_eq!(
            batches(&queue)[0]["markdown"].as_str().unwrap().as_bytes(),
            copied.as_bytes()
        );
        assert_eq!(serde_json::to_value(&app.session).unwrap(), before);
        assert!(app.dirty);
        assert!(!app.should_quit);
        assert!(app.pending_stdout_output.is_none());
        assert!(copied.contains("[BLOCKING]"));
        assert!(copied.contains("src/main.rs:~2-~4"));
        assert!(copied.contains("(commit abc1234)"));
        assert!(copied.contains("Summary: Batch summary"));
        assert!(copied.contains("Please address these exact comments."));
        if pr_mode {
            assert!(copied.contains("URL: https://github.com/owner/repo/pull/42"));
            assert!(copied.contains("Head: abcdef0"));
            assert!(copied.contains("Unresolved upstream feedback"));
        } else {
            assert!(copied.contains("Reviewing commits: abc1234, def5678"));
        }
    }
}

#[test]
fn send_agent_suppresses_unchanged_repeats_and_force_resends_full_export() {
    let temp = tempfile::tempdir().unwrap();
    let queue = temp.path().join("queue");
    let mut app = review(&queue);
    command(&mut app, "send-agent");
    let first_id = batches(&queue)[0]["id"].clone();
    handle_diff_action(&mut app, Action::SendToAgent);
    command(&mut app, "send-agent");
    assert_eq!(batches(&queue).len(), 1);
    assert!(
        app.message
            .as_ref()
            .unwrap()
            .content
            .contains(first_id.as_str().unwrap())
    );
    assert!(
        app.message
            .as_ref()
            .unwrap()
            .content
            .contains(":send-agent!")
    );
    command(&mut app, "send-agent!");
    let queued = batches(&queue);
    assert_eq!(queued.len(), 2);
    assert_ne!(queued[0]["id"], queued[1]["id"]);
    assert_eq!(queued[0]["markdown"], queued[1]["markdown"]);
    app.session.session_notes = Some("Updated summary".into());
    command(&mut app, "send-agent");
    assert_eq!(batches(&queue).len(), 3);
    assert!(
        batches(&queue)
            .iter()
            .all(|batch| batch["markdown"].as_str().unwrap().contains("Fix café"))
    );
}

#[test]
fn send_agent_rejects_selector_empty_review_and_failed_path_without_advancing_guard() {
    let temp = tempfile::tempdir().unwrap();
    let queue = temp.path().join("queue");
    let mut app = review(&queue);
    app.input_mode = InputMode::CommitSelect;
    command(&mut app, "send-agent");
    assert_eq!(app.input_mode, InputMode::CommitSelect);
    assert!(
        app.message
            .as_ref()
            .unwrap()
            .content
            .contains("Open a review")
    );
    assert!(!queue.exists());
    app.input_mode = InputMode::Normal;
    app.agent_queue_dir = Some("relative".into());
    let before = serde_json::to_value(&app.session).unwrap();
    command(&mut app, "send-agent");
    assert!(
        app.message
            .as_ref()
            .unwrap()
            .content
            .contains("absolute path")
    );
    assert!(app.last_agent_batch.is_none());
    assert_eq!(serde_json::to_value(&app.session).unwrap(), before);
    app.agent_queue_dir = Some(queue.clone());
    command(&mut app, "send-agent");
    assert_eq!(batches(&queue).len(), 1);
    let empty_queue = temp.path().join("empty");
    let mut empty = app_with(vec![]);
    empty.agent_queue_dir = Some(empty_queue.clone());
    command(&mut empty, "send-agent");
    assert!(!empty_queue.exists());
    assert!(empty.last_agent_batch.is_none());
    assert!(
        empty
            .message
            .as_ref()
            .unwrap()
            .content
            .contains("No comments")
    );
}

#[test]
fn send_agent_bindings_are_review_only_and_never_capture_text_or_selector_actions() {
    let mut app = app_with(Vec::new());
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    for panel in [
        FocusedPanel::Diff,
        FocusedPanel::FileList,
        FocusedPanel::CommitSelector,
        FocusedPanel::Comments,
    ] {
        app.focused_panel = panel;
        assert_eq!(map_panel_key(&app, key, true), Some(Action::SendToAgent));
    }
    for mode in [
        InputMode::Comment,
        InputMode::Command,
        InputMode::Search,
        InputMode::Help,
        InputMode::Summary,
        InputMode::Confirm,
        InputMode::VisualSelect,
        InputMode::CommitSelect,
    ] {
        app.input_mode = mode;
        app.target_tab = TargetTab::PullRequests;
        assert_eq!(map_panel_key(&app, key, true), None);
    }
    app.input_mode = InputMode::Normal;
    app.focused_panel = FocusedPanel::FileList;
    app.begin_file_tree_prompt(FileTreePrompt::Include);
    assert_eq!(map_panel_key(&app, key, true), None);
    app.cancel_file_tree_prompt();
    app.panel_bindings =
        PanelBindings::from_config(&toml::from_str("[keybindings]\nsend_to_agent=[]").unwrap())
            .unwrap();
    assert_eq!(map_panel_key(&app, key, true), None);
}

#[test]
fn saving_comment_does_not_enqueue() {
    let temp = tempfile::tempdir().unwrap();
    let queue = temp.path().join("queue");
    let mut app = review(&queue);
    app.session_path = Some(temp.path().join("session.json"));
    app.enter_review_comment_mode();
    app.comment_buffer = "A saved review comment".into();
    app.save_comment();
    assert!(app.session.has_comments());
    assert!(!queue.exists());
    assert!(app.last_agent_batch.is_none());
}
