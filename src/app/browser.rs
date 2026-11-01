use std::sync::mpsc::{self, TryRecvError};

use super::{App, DiffSource, InputMode, TargetTab};

impl App {
    pub fn open_pr_in_browser(&mut self) {
        self.open_pr_in_browser_with(|url| {
            opener::open_browser(url).map_err(|err| err.to_string())
        });
    }

    fn open_pr_in_browser_with(
        &mut self,
        open: impl FnOnce(&str) -> Result<(), String> + Send + 'static,
    ) {
        let raw_url = if self.input_mode == InputMode::CommitSelect {
            // A selector with no PR under its cursor must never fall back to
            // the previous review, which may still be stored in diff_source.
            if self.target_tab != TargetTab::PullRequests || self.pr_filter_editing() {
                return;
            }
            let Some(pr) = self.pr_tab.cursor_pr() else {
                return;
            };
            &pr.url
        } else if let DiffSource::PullRequest(pr) = &self.diff_source {
            &pr.url
        } else {
            self.set_warning("Open PR in browser only applies in PR mode");
            return;
        };
        // Forge metadata is external input. Never dispatch a file, custom
        // protocol, or command-line option to the operating system's handler.
        let Ok(url) = url::Url::parse(raw_url) else {
            self.set_warning("PR has no valid HTTP(S) URL to open");
            return;
        };
        if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
            self.set_warning("PR has no valid HTTP(S) URL to open");
            return;
        }
        if self.browser_open_rx.is_some() {
            self.set_message("Browser launch is already in progress");
            return;
        }

        // Even a desktop launcher can block. Keep all OS dispatch off the TUI
        // thread; opener handles platform arguments and suppresses child stdio.
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("pr-browser".into())
            .spawn(move || {
                let _ = tx.send(open(url.as_str()));
            }) {
            Ok(_) => {
                self.browser_open_rx = Some(rx);
                self.set_message("Sending PR URL to default browser...");
            }
            Err(err) => self.set_warning(format!("Could not open PR in browser: {err}")),
        }
    }

    /// Returns true when a launch result changes the message bar.
    pub fn poll_browser_open(&mut self) -> bool {
        let Some(rx) = &self.browser_open_rx else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("browser launcher stopped unexpectedly".into()),
        };
        self.browser_open_rx = None;
        match result {
            // Dispatch success cannot prove a browser actually displayed the
            // page (in particular, xdg-open can fail after dispatch on Linux).
            Ok(()) => self.set_message("PR URL sent to default browser"),
            Err(err) => self.set_warning(format!("Could not open PR in browser: {err}")),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::app::tests::render_perf_tests::app_with;
    use crate::forge::traits::{ForgeRepository, PrSessionKey};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn pr_app(url: &str) -> App {
        let mut app = app_with(Vec::new());
        app.diff_source = DiffSource::PullRequest(Box::new(PullRequestDiffSource {
            key: PrSessionKey {
                repository: ForgeRepository::github("github.com", "owner", "repo"),
                number: 42,
                head_sha: "reviewed-head".into(),
            },
            base_sha: "base".into(),
            title: "Reviewed PR".into(),
            url: url.into(),
            head_ref_name: "feature".into(),
            base_ref_name: "main".into(),
            state: "OPEN".into(),
            closed: false,
            merged: false,
        }));
        app
    }

    fn wait_for_result(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.poll_browser_open() {
            assert!(Instant::now() < deadline, "browser result did not arrive");
            std::thread::yield_now();
        }
        assert!(app.browser_open_rx.is_none());
    }

    fn selector_app() -> App {
        use crate::forge::selector::PullRequestsTab;
        use crate::forge::traits::{PullRequestListScope, PullRequestSummary};

        // Keep an old reviewed PR around to catch accidental fallback to it.
        let mut app = pr_app("https://github.com/owner/repo/pull/42");
        app.input_mode = InputMode::CommitSelect;
        app.target_tab = TargetTab::PullRequests;
        app.pr_tab = PullRequestsTab::Loaded {
            repository: ForgeRepository::github("github.com", "owner", "repo"),
            rows: [93, 12, 45]
                .into_iter()
                .map(|number| PullRequestSummary {
                    repository: ForgeRepository::github("github.com", "owner", "repo"),
                    number,
                    title: if number == 12 { "other" } else { "match" }.into(),
                    author: None,
                    head_ref_name: "feature".into(),
                    base_ref_name: "main".into(),
                    updated_at: None,
                    url: format!("https://github.com/owner/repo/pull/{number}"),
                    state: "OPEN".into(),
                    is_draft: false,
                })
                .collect(),
            has_more: true,
            loading_more: false,
            filter: String::new(),
            scope: PullRequestListScope::Open,
            cursor: 0,
            scroll_offset: 0,
        };
        app
    }

    #[test]
    fn selector_browser_opens_the_visible_cursor_pr_without_changing_review() {
        for (filter, expected) in [("", 12), ("match", 45)] {
            let mut app = selector_app();
            app.pr_tab.set_filter(filter.into());
            app.pr_tab.cursor_down();
            let view = app.pr_tab.view();
            let selected_url = view.rows[view.cursor].summary.url.clone();
            assert!(selected_url.ends_with(&format!("/{expected}")));
            let (tx, rx) = mpsc::channel();
            app.open_pr_in_browser_with(move |url| {
                tx.send(url.to_owned()).unwrap();
                Ok(())
            });
            // A later cursor move must not change the snapshotted request.
            app.pr_tab.cursor_up();
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                selected_url
            );
            wait_for_result(&mut app);
            assert_eq!(app.input_mode, InputMode::CommitSelect);
            assert!(!app.dirty);
            let DiffSource::PullRequest(pr) = &app.diff_source else {
                panic!("review changed")
            };
            assert!(pr.url.ends_with("/42"));
        }
    }

    #[test]
    fn selector_browser_does_nothing_without_a_selected_pr_or_during_filter_entry() {
        use crate::forge::selector::PullRequestsTab;
        for case in 0..10 {
            let mut app = selector_app();
            match case {
                0 => app.target_tab = TargetTab::Local,
                1 => app.target_tab = TargetTab::Sessions,
                2 => app.pr_tab = PullRequestsTab::new(None),
                3..=5 => {
                    app.pr_tab = PullRequestsTab::new(Some(ForgeRepository::github(
                        "github.com",
                        "owner",
                        "repo",
                    )));
                    if case >= 4 {
                        app.pr_tab.start_initial_load();
                    }
                    if case == 5 {
                        app.pr_tab.apply_initial_load(Err("offline".into()));
                    }
                }
                6 => {
                    if let PullRequestsTab::Loaded { rows, has_more, .. } = &mut app.pr_tab {
                        rows.clear();
                        *has_more = false;
                    }
                }
                7 => app.pr_tab.set_filter("no match".into()),
                8 => {
                    for _ in 0..3 {
                        app.pr_tab.cursor_down();
                    }
                }
                9 => app.begin_pr_filter(),
                _ => unreachable!(),
            }
            app.open_pr_in_browser_with(|_| panic!("must not open an old review"));
            assert!(app.browser_open_rx.is_none(), "case {case}");
            assert!(app.message.is_none(), "case {case}");
        }
    }

    #[test]
    fn selector_browser_reuses_url_validation_and_reports_launch_errors() {
        use crate::forge::selector::PullRequestsTab;
        let mut app = selector_app();
        if let PullRequestsTab::Loaded { rows, .. } = &mut app.pr_tab {
            rows[0].url = "file:///tmp/not-a-pr".into();
        }
        // Dispatch through the same handler used by the selector event loop.
        crate::handler::handle_commit_select_action(
            &mut app,
            crate::input::Action::OpenPrInBrowser,
        );
        assert!(app.browser_open_rx.is_none());
        assert!(app.message.as_ref().unwrap().content.contains("HTTP(S)"));

        let mut app = selector_app();
        app.open_pr_in_browser_with(|_| Err("browser unavailable".into()));
        wait_for_result(&mut app);
        let message = &app.message.as_ref().unwrap().content;
        assert!(message.contains("browser unavailable"));
        assert!(
            !message.contains(":copy-url"),
            "must not suggest copying an old reviewed PR"
        );
    }

    #[test]
    fn open_pr_browser_sends_reviewed_url_unchanged_and_reports_dispatch() {
        for url in [
            "https://github.com/owner/repo/pull/42?x=a&y=$(not-a-command)#discussion",
            "http://forge.example/team/repo/-/merge_requests/42",
        ] {
            let mut app = pr_app(url);
            let (tx, rx) = mpsc::channel();
            app.open_pr_in_browser_with(move |url| {
                tx.send(url.to_owned()).unwrap();
                Ok(())
            });
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), url);
            wait_for_result(&mut app);
            assert_eq!(
                app.message.as_ref().unwrap().content,
                "PR URL sent to default browser"
            );
            assert!(!app.dirty);
            assert_eq!(app.input_mode, InputMode::Normal);
        }
    }

    #[test]
    fn open_pr_browser_dispatches_the_validated_normalized_url() {
        let mut app = pr_app("HTTPS://Forge.Example/pull/42?note=a b");
        let (tx, rx) = mpsc::channel();
        app.open_pr_in_browser_with(move |url| {
            tx.send(url.to_owned()).unwrap();
            Ok(())
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "https://forge.example/pull/42?note=a%20b"
        );
        wait_for_result(&mut app);
    }

    #[test]
    fn open_pr_browser_rejects_local_missing_invalid_and_non_web_urls_before_launch() {
        let mut local = app_with(Vec::new());
        local.open_pr_in_browser_with(|_| panic!("must not open browser in local review"));
        assert!(local.browser_open_rx.is_none());
        assert!(
            local
                .message
                .as_ref()
                .unwrap()
                .content
                .contains("only applies in PR mode")
        );
        for url in [
            "",
            "   ",
            "not a url",
            "https://",
            "file:///tmp/review",
            "javascript:alert(1)",
            "mailto:me@example.com",
            "--help",
        ] {
            let mut app = pr_app(url);
            app.open_pr_in_browser_with(|_| panic!("must not dispatch an invalid URL"));
            assert!(app.browser_open_rx.is_none(), "{url}");
            assert!(
                app.message.as_ref().unwrap().content.contains("URL"),
                "{url}"
            );
        }
    }

    #[test]
    fn open_pr_browser_reports_launch_errors_and_allows_retry() {
        let mut app = pr_app("https://github.com/owner/repo/pull/42");
        app.open_pr_in_browser_with(|_| Err("no browser launcher available".into()));
        wait_for_result(&mut app);
        let message = &app.message.as_ref().unwrap().content;
        assert!(message.contains("Could not open PR in browser"));
        assert!(message.contains("no browser launcher available"));
        app.open_pr_in_browser_with(|_| Ok(()));
        wait_for_result(&mut app);
        assert_eq!(
            app.message.as_ref().unwrap().content,
            "PR URL sent to default browser"
        );
    }

    #[test]
    fn open_pr_browser_is_nonblocking_and_deduplicates_pending_launches() {
        let mut app = pr_app("https://github.com/owner/repo/pull/42");
        let (release, wait) = mpsc::channel();
        app.open_pr_in_browser_with(move |_| {
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        });
        assert!(!app.poll_browser_open());
        app.open_pr_in_browser_with(|_| panic!("must not duplicate a pending launch"));
        release.send(()).unwrap();
        wait_for_result(&mut app);
    }

    #[test]
    fn open_pr_browser_handles_disconnected_worker() {
        let mut app = pr_app("https://github.com/owner/repo/pull/42");
        let (tx, rx) = mpsc::channel();
        drop(tx);
        app.browser_open_rx = Some(rx);
        assert!(app.poll_browser_open());
        assert!(app.browser_open_rx.is_none());
        assert!(
            app.message
                .as_ref()
                .unwrap()
                .content
                .contains("Could not open PR in browser")
        );
        assert!(!app.poll_browser_open());
    }
}
