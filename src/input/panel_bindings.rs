//! The six configurable review-navigation actions, not a general mode keymap.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keybindings::{Action, map_file_tree_mode, map_global_key, map_key_to_action};
use crate::app::InputMode;
use crate::config::{AppConfig, DEFAULT_LEADER_KEY};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Binding {
    leader: bool,
    code: KeyCode,
    modifiers: KeyModifiers,
}

#[derive(Debug, Clone)]
pub struct PanelBindings(Vec<(Action, Vec<Binding>)>);

fn actions() -> [(&'static str, Action, char); 6] {
    [
        ("focus_left", Action::FocusLeft, 'h'),
        ("focus_right", Action::FocusRight, 'l'),
        ("focus_up", Action::FocusUp, 'k'),
        ("focus_down", Action::FocusDown, 'j'),
        ("toggle_file_list", Action::ToggleFileList, 'e'),
        ("toggle_single_file_view", Action::ToggleSingleFileView, 'f'),
    ]
}

impl Default for PanelBindings {
    fn default() -> Self {
        Self(
            actions()
                .into_iter()
                .map(|(_, action, c)| {
                    (
                        action,
                        vec![Binding {
                            leader: true,
                            code: KeyCode::Char(c),
                            modifiers: KeyModifiers::NONE,
                        }],
                    )
                })
                .collect(),
        )
    }
}

impl PanelBindings {
    pub fn from_config(config: &AppConfig) -> Result<Self, String> {
        let mut result = Self::default();
        let Some(overrides) = &config.keybindings else {
            return Ok(result);
        };
        for (name, values) in overrides {
            let Some(index) = actions().iter().position(|(n, _, _)| n == name) else {
                return Err(format!("unknown action '{name}'"));
            };
            result.0[index].1 = values
                .iter()
                .map(|value| parse_binding(value).map_err(|err| format!("{name}: {err}")))
                .collect::<Result<_, _>>()?;
        }
        let leader = config.leader.unwrap_or(DEFAULT_LEADER_KEY);
        let mut seen = Vec::new();
        // Validate only the final map, so two actions may exchange defaults.
        for (action, bindings) in &result.0 {
            for binding in bindings {
                let key = KeyEvent::new(binding.code, binding.modifiers);
                let reserved_global = binding.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(binding.code, KeyCode::Char('c' | 'l'));
                let reserved = if binding.leader {
                    // These fixed leader commands intentionally match by code.
                    matches!(binding.code, KeyCode::Char('c' | 's' | 'n'))
                } else {
                    equivalent_events(binding).into_iter().any(|key| {
                        map_file_tree_mode(key, leader) != Action::None
                            || map_key_to_action(key, InputMode::Normal, leader) != Action::None
                    })
                };
                if reserved || reserved_global || map_global_key(key) != Action::None {
                    return Err(format!(
                        "{} conflicts with a fixed binding",
                        binding.label(leader)
                    ));
                }
                if seen.contains(binding) {
                    return Err(format!(
                        "{} is assigned more than once ({action:?})",
                        binding.label(leader)
                    ));
                }
                seen.push(binding.clone());
            }
        }
        Ok(result)
    }

    /// Call only in normal review navigation, never in a text-input substate.
    pub fn resolve(&self, key: KeyEvent, leader: bool) -> Option<Action> {
        let (code, modifiers) = normalize(key.code, key.modifiers);
        self.0
            .iter()
            .find(|(_, bindings)| {
                bindings.contains(&Binding {
                    leader,
                    code,
                    modifiers,
                })
            })
            .map(|(action, _)| action.clone())
    }

    pub fn labels(&self, action: Action, leader: char) -> String {
        self.0
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .filter(|(_, bindings)| !bindings.is_empty())
            .map(|(_, bindings)| {
                bindings
                    .iter()
                    .map(|binding| binding.label(leader))
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_else(|| "unbound".to_string())
    }
}

impl Binding {
    fn label(&self, leader: char) -> String {
        let mut label = if self.leader {
            if leader == ' ' {
                "Space ".to_string()
            } else if leader.is_whitespace() || leader.is_control() {
                format!("{} ", leader.escape_default())
            } else {
                leader.to_string()
            }
        } else {
            String::new()
        };
        for (modifier, name) in [
            (KeyModifiers::CONTROL, "Ctrl-"),
            (KeyModifiers::ALT, "Alt-"),
            (KeyModifiers::SHIFT, "Shift-"),
        ] {
            if self.modifiers.contains(modifier) {
                label.push_str(name);
            }
        }
        label.push_str(&match self.code {
            KeyCode::Char(' ') => "Space".into(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::F(n) => format!("F{n}"),
            code => format!("{code:?}"),
        });
        label
    }
}

// Validate every encoding we resolve, including code-only fixed mappings on
// pre-normalized events (for example Ctrl-Alt-[ still means PreviousHunk).
fn equivalent_events(binding: &Binding) -> Vec<KeyEvent> {
    let mut keys = vec![KeyEvent::new(binding.code, binding.modifiers)];
    let chars: &[char] = match binding.code {
        KeyCode::Tab => &['i'],
        KeyCode::Enter => &['j', 'm'],
        KeyCode::Esc => &['['],
        KeyCode::Backspace => &['h', '?'],
        _ => &[],
    };
    for c in chars {
        keys.push(KeyEvent::new(
            KeyCode::Char(*c),
            binding.modifiers | KeyModifiers::CONTROL,
        ));
    }
    if let KeyCode::Char(c) = binding.code {
        keys.push(KeyEvent::new(
            KeyCode::Char(c),
            binding.modifiers | KeyModifiers::SHIFT,
        ));
        if binding.modifiers.contains(KeyModifiers::CONTROL) {
            keys.push(KeyEvent::new(
                KeyCode::Char(c.to_ascii_uppercase()),
                binding.modifiers | KeyModifiers::SHIFT,
            ));
        }
    }
    keys
}

fn parse_binding(raw: &str) -> Result<Binding, String> {
    let (leader, mut chord) = match raw.strip_prefix("<leader>") {
        Some(chord) => (true, chord),
        None => (false, raw),
    };
    if chord.starts_with('<') && chord.ends_with('>') {
        chord = &chord[1..chord.len() - 1];
    }
    let mut modifiers = KeyModifiers::NONE;
    while let Some((prefix, rest)) = chord.split_once('-') {
        let modifier = match prefix.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => break,
        };
        if modifiers.contains(modifier) {
            return Err(format!("duplicate modifier in '{raw}'"));
        }
        modifiers.insert(modifier);
        chord = rest;
    }
    let code = match chord.to_ascii_lowercase().as_str() {
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdown" => KeyCode::PageDown,
        _ => {
            let mut chars = chord.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() && !c.is_whitespace() => KeyCode::Char(c),
                _ if chord.starts_with(['f', 'F']) => {
                    let n = chord[1..]
                        .parse::<u8>()
                        .ok()
                        .filter(|n| (1..=24).contains(n))
                        .ok_or_else(|| format!("invalid key '{raw}'"))?;
                    KeyCode::F(n)
                }
                _ => return Err(format!("expected one key or chord, got '{raw}'")),
            }
        }
    };
    if modifiers.contains(KeyModifiers::SHIFT)
        && matches!(code, KeyCode::Char(c) if !c.is_ascii_alphabetic())
    {
        return Err(format!("use the resulting character instead of '{raw}'"));
    }
    let (code, modifiers) = normalize(code, modifiers);
    Ok(Binding {
        leader,
        code,
        modifiers,
    })
}

// Account for legacy terminal encodings as well as shifted character events.
// Conservatively reject collisions even if an enhanced keyboard can distinguish them.
fn normalize(mut code: KeyCode, mut modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    if let KeyCode::Char(c) = code {
        if modifiers.contains(KeyModifiers::CONTROL) {
            code = KeyCode::Char(c.to_ascii_lowercase());
            modifiers.remove(KeyModifiers::SHIFT);
            let equivalent = match code {
                KeyCode::Char('i') => Some(KeyCode::Tab),
                KeyCode::Char('m' | 'j') => Some(KeyCode::Enter),
                KeyCode::Char('[') => Some(KeyCode::Esc),
                KeyCode::Char('h' | '?') => Some(KeyCode::Backspace),
                _ => None,
            };
            if let Some(equivalent) = equivalent {
                code = equivalent;
                modifiers.remove(KeyModifiers::CONTROL);
            }
        } else {
            if modifiers.contains(KeyModifiers::SHIFT) {
                code = KeyCode::Char(c.to_ascii_uppercase());
            }
            modifiers.remove(KeyModifiers::SHIFT);
        }
    }
    if code == KeyCode::Tab && modifiers.contains(KeyModifiers::SHIFT) {
        code = KeyCode::BackTab;
    }
    if code == KeyCode::BackTab {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    (code, modifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> AppConfig {
        toml::from_str(text).unwrap()
    }
    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn panel_defaults_resolve_all_six_only_after_leader() {
        let map = PanelBindings::default();
        for (_, action, c) in actions() {
            assert_eq!(map.resolve(key(c), true), Some(action.clone()));
            assert_eq!(map.resolve(key(c), false), None);
            assert_eq!(map.labels(action, ';'), format!(";{c}"));
        }
    }

    #[test]
    fn panel_replacements_aliases_and_unbinding_remove_old_defaults() {
        let map = PanelBindings::from_config(&config(
            "[keybindings]\nfocus_left=['Alt-Left','<leader>a']\ntoggle_file_list=[]",
        ))
        .unwrap();
        assert_eq!(map.resolve(key('h'), true), None);
        assert_eq!(map.resolve(key('a'), true), Some(Action::FocusLeft));
        assert_eq!(
            map.resolve(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT), false),
            Some(Action::FocusLeft)
        );
        assert_eq!(
            map.resolve(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL), false),
            None
        );
        assert_eq!(map.resolve(key('e'), true), None);
        assert_eq!(map.labels(Action::FocusLeft, ','), "Alt-Left / ,a");
        assert_eq!(map.labels(Action::ToggleFileList, ','), "unbound");
        assert_eq!(map.resolve(key('l'), true), Some(Action::FocusRight));
    }

    #[test]
    fn panel_effective_validation_allows_swaps_and_free_default_reuse() {
        let map = PanelBindings::from_config(&config(
            "leader=','\n[keybindings]\nfocus_left=['<leader>l']\nfocus_right=['<leader>h']",
        ))
        .unwrap();
        assert_eq!(map.resolve(key('l'), true), Some(Action::FocusLeft));
        assert_eq!(map.resolve(key('h'), true), Some(Action::FocusRight));
        assert_eq!(map.labels(Action::FocusLeft, ','), ",l");
        assert!(
            PanelBindings::from_config(&config(
                "[keybindings]\nfocus_left=[]\nfocus_right=['<leader>h']"
            ))
            .is_ok()
        );
    }

    #[test]
    fn panel_conflicts_invalid_sequences_and_unknown_actions_are_rejected() {
        for text in [
            "focus_left=['<leader>l']",
            "focus_left=['Alt-Left','Alt-Left']",
            "focus_left=['Alt-Left']\nfocus_right=['Alt-Left']",
            "typo=[]",
            "focus_left=['jj']",
            "focus_left=['<leader>gg']",
            "focus_left=['']",
            "focus_left=['Ctrl-C']",
            "focus_left=['Ctrl-Alt-c']",
            "focus_left=['<leader>Ctrl-C']",
            "focus_left=['Ctrl-L']",
            "focus_left=['<leader>Ctrl-L']",
            "focus_left=[';']",
            "focus_left=['j']",
            "focus_left=['i']",
            "focus_left=['E']",
            "focus_left=['Alt-I']",
            "focus_left=['Alt-G']",
            "focus_left=['Ctrl-d']",
            "focus_left=['<leader>c']",
            "focus_left=['<leader>Alt-s']",
            "focus_left=['<leader>n']",
            "focus_left=['Shift-1']",
            "focus_left=['Shift-;']",
            "focus_left=['Shift--']",
            "focus_left=['Shift-é']",
            "focus_left=['Ctrl-i']",
            "focus_left=['Ctrl-m']",
            "focus_left=['Ctrl-[']",
            "focus_left=['Ctrl-Ctrl-a']",
            "focus_left=['F25']",
            "focus_left=['Alt-Esc']",
            "focus_left=['Ctrl-Alt-[']",
        ] {
            assert!(
                PanelBindings::from_config(&config(&format!("[keybindings]\n{text}"))).is_err(),
                "{text}"
            );
        }
        assert!(
            PanelBindings::from_config(&config("leader=','\n[keybindings]\nfocus_left=[',']"))
                .is_err()
        );
    }

    #[test]
    fn panel_shift_and_terminal_equivalents_match_and_collide() {
        let map = PanelBindings::from_config(&config(
            "[keybindings]\nfocus_left=['<leader>Shift-a','<leader>Alt-Backspace']",
        ))
        .unwrap();
        assert_eq!(
            map.resolve(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT), true),
            Some(Action::FocusLeft)
        );
        assert_eq!(map.resolve(key('A'), true), Some(Action::FocusLeft));
        assert_eq!(map.resolve(key('a'), true), None);
        assert_eq!(
            map.resolve(
                KeyEvent::new(
                    KeyCode::Char('h'),
                    KeyModifiers::ALT | KeyModifiers::CONTROL
                ),
                true
            ),
            Some(Action::FocusLeft)
        );
        for values in [
            "'A','Shift-a'",
            "'Tab','Ctrl-i'",
            "'Enter','Ctrl-m'",
            "'Backspace','Ctrl-h'",
            "'BackTab','Shift-Tab'",
        ] {
            // Build each leader spelling individually (not the closing quote).
            let values = values
                .split(',')
                .map(|s| format!("'<leader>{}'", s.trim_matches('\'')))
                .collect::<Vec<_>>()
                .join(",");
            assert!(
                PanelBindings::from_config(&config(&format!(
                    "[keybindings]\nfocus_left=[{values}]"
                )))
                .is_err(),
                "{values}"
            );
        }
    }

    #[test]
    fn panel_space_leader_labels_and_changed_leader_conflicts() {
        let map = PanelBindings::from_config(&config(
            "leader=' '\n[keybindings]\nfocus_left=['<leader>a']",
        ))
        .unwrap();
        assert_eq!(map.labels(Action::FocusLeft, ' '), "Space a");
        assert_eq!(map.resolve(key('a'), true), Some(Action::FocusLeft));
        assert!(
            PanelBindings::from_config(&config("leader=' '\n[keybindings]\nfocus_left=['Space']"))
                .is_err()
        );
        assert!(
            PanelBindings::from_config(&config("leader=','\n[keybindings]\nfocus_left=[';']"))
                .is_ok()
        );
    }

    #[test]
    fn panel_named_keys_and_free_characters_parse() {
        for spelling in [
            "Alt-Left",
            "Alt-Right",
            "F12",
            "<leader>Space",
            "<leader>PageDown",
            "<leader>-",
            "<leader>é",
            "<Alt-Up>",
        ] {
            assert!(parse_binding(spelling).is_ok(), "{spelling}");
        }
    }
}
