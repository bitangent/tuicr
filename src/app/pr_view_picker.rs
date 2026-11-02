//! Small, type-to-search picker for the selector's configured PR views.
use ratatui::widgets::ListState;

use super::{App, InputMode, TargetTab};
use crate::forge::traits::PullRequestView;
use crate::input::Action;

#[derive(Debug)]
pub struct PrViewPicker {
    pub candidates: Vec<PullRequestView>,
    pub query: String,
    pub list_state: ListState,
}

impl PrViewPicker {
    pub fn filtered_indices(&self) -> Vec<usize> {
        let needle = self.query.to_lowercase();
        self.candidates
            .iter()
            .enumerate()
            .filter(|(_, view)| view.name.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn selected_view(&self) -> Option<&PullRequestView> {
        self.filtered_indices()
            .get(self.list_state.selected()?)
            .and_then(|index| self.candidates.get(*index))
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_indices().len();
        if len == 0 {
            self.list_state.select(None);
        } else {
            let selected = self
                .list_state
                .selected()
                .unwrap_or(0)
                .saturating_add_signed(delta)
                .min(len - 1);
            self.list_state.select(Some(selected));
        }
    }
}

impl App {
    pub fn open_pr_view_picker(&mut self) {
        if self.input_mode != InputMode::CommitSelect
            || self.target_tab != TargetTab::PullRequests
            || self.pr_tab.repository().is_none()
            || self.pr_open_state.is_some()
        {
            return;
        }
        let selected = self
            .pr_views
            .iter()
            .position(|view| Some(view) == self.pr_tab.active_view())
            .unwrap_or(0);
        self.pr_view_picker = Some(PrViewPicker {
            candidates: self.pr_views.clone(),
            query: String::new(),
            list_state: ListState::default().with_selected(Some(selected)),
        });
    }

    pub fn handle_pr_view_picker_action(&mut self, action: Action) {
        let Some(picker) = self.pr_view_picker.as_mut() else {
            return;
        };
        match action {
            Action::ExitMode => {
                self.pr_view_picker = None;
            }
            Action::SubmitInput => {
                let Some(view) = picker.selected_view().cloned() else {
                    return;
                };
                self.pr_view_picker = None;
                self.select_pr_view(view);
            }
            Action::CursorDown(n) => picker.move_selection(n as isize),
            Action::CursorUp(n) => picker.move_selection(-(n as isize)),
            Action::InsertChar(c) => {
                picker.query.push(c);
                picker.list_state = ListState::default().with_selected(Some(0));
            }
            Action::Paste(text) => {
                picker
                    .query
                    .extend(text.chars().filter(|c| !c.is_control()));
                picker.list_state = ListState::default().with_selected(Some(0));
            }
            Action::DeleteChar => {
                picker.query.pop();
                picker.list_state = ListState::default().with_selected(Some(0));
            }
            Action::ClearLine | Action::DeleteWord => {
                picker.query.clear();
                picker.list_state = ListState::default().with_selected(Some(0));
            }
            _ => {}
        }
    }

    pub fn select_pr_view(&mut self, view: PullRequestView) {
        let Some((repository, view)) = self.pr_tab.select_view_and_start_reload(view) else {
            return;
        };
        self.pr_filter_draft = None;
        self.set_message(format!("PR list: {}", view.name));
        self.spawn_pr_initial_load(
            repository,
            self.repo_url_override.clone(),
            self.canonical_resolved,
            view,
        );
    }
}
