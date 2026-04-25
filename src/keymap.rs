//! 設定可能なキーバインドの中核。
//!
//! - [`Action`] : ユーザに見える「動作」の列挙。
//! - [`KeyChord`] : 1 つのキー入力 (modifiers + code) を表す。文字列との
//!   相互変換を持つ。
//! - [`KeyMap`] : 4 つのスコープ (global / pane / file_tree / preview)
//!   ごとに `KeyChord -> Action` を持つルックアップ表。
//!
//! TOML の `[keybindings.<scope>]` テーブルを読んでデフォルトに上書き
//! マージする。値に `"none"` を指定するとそのキーを無効化できる。

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Deserializer};

// ─── Action ────────────────────────────────────────────────

/// 設定可能なアクション。Alt+1..9 (タブジャンプ) や PTY 透過は
/// 設定対象外なのでここには無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // global
    Quit,
    NewTab,
    NextTab,
    PrevTab,
    RenameTab,
    ToggleStatusBar,
    ToggleFileTree,
    SwapLayout,
    FocusNextPane,
    FocusPrevPane,
    /// 選択中ならクリップボードへコピーして吸収。それ以外は false を
    /// 返して呼び出し側 (PTY 転送) にフォールスルーさせる。
    CopySelectionOrPassthrough,

    // pane
    SplitVertical,
    SplitHorizontal,
    /// プレビュー閉じ → ペイン閉じ → タブ閉じ、の順で利用可能なものを実行。
    ClosePaneOrTab,
    /// 選択 / scrollback 状態があれば解除して吸収。それ以外は PTY へ。
    ClearSelectionOrPassthrough,

    // file_tree
    FileTreeDown,
    FileTreeUp,
    FileTreeOpen,
    FileTreeToggleHidden,
    FileTreeBlur,

    // preview
    PreviewScrollDown,
    PreviewScrollUp,
    PreviewPageDown,
    PreviewPageUp,
    PreviewScrollLeft,
    PreviewScrollRight,
    PreviewScrollHome,
    PreviewClose,
    PreviewBlur,
}

impl Action {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        Some(match s {
            "quit" => Self::Quit,
            "new_tab" => Self::NewTab,
            "next_tab" => Self::NextTab,
            "prev_tab" => Self::PrevTab,
            "rename_tab" => Self::RenameTab,
            "toggle_status_bar" => Self::ToggleStatusBar,
            "toggle_file_tree" => Self::ToggleFileTree,
            "swap_layout" => Self::SwapLayout,
            "focus_next_pane" => Self::FocusNextPane,
            "focus_prev_pane" => Self::FocusPrevPane,
            "copy_selection_or_passthrough" => Self::CopySelectionOrPassthrough,
            "split_vertical" => Self::SplitVertical,
            "split_horizontal" => Self::SplitHorizontal,
            "close_pane_or_tab" => Self::ClosePaneOrTab,
            "clear_selection_or_passthrough" => Self::ClearSelectionOrPassthrough,
            "filetree_down" => Self::FileTreeDown,
            "filetree_up" => Self::FileTreeUp,
            "filetree_open" => Self::FileTreeOpen,
            "filetree_toggle_hidden" => Self::FileTreeToggleHidden,
            "filetree_blur" => Self::FileTreeBlur,
            "preview_scroll_down" => Self::PreviewScrollDown,
            "preview_scroll_up" => Self::PreviewScrollUp,
            "preview_page_down" => Self::PreviewPageDown,
            "preview_page_up" => Self::PreviewPageUp,
            "preview_scroll_left" => Self::PreviewScrollLeft,
            "preview_scroll_right" => Self::PreviewScrollRight,
            "preview_scroll_home" => Self::PreviewScrollHome,
            "preview_close" => Self::PreviewClose,
            "preview_blur" => Self::PreviewBlur,
            _ => return None,
        })
    }
}

// ─── KeyChord ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub mods: KeyModifiers,
    pub code: KeyCode,
}

impl KeyChord {
    pub const fn new(mods: KeyModifiers, code: KeyCode) -> Self {
        Self { mods, code }
    }

    /// `"ctrl+d"` のような文字列を chord にパースする。
    /// `+` 区切り、修飾キー (ctrl/alt/shift) は順序非依存、大文字小文字
    /// 不問。最後のトークンがキー本体。文字キーは単一文字、特殊キーは
    /// `enter` / `esc` / `pageup` / `f5` 等の名前。
    pub fn parse(s: &str) -> Option<Self> {
        let mut mods = KeyModifiers::NONE;
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
        if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        let (key_token, mod_tokens) = parts.split_last()?;
        for m in mod_tokens {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
                "alt" | "meta" => mods |= KeyModifiers::ALT,
                "shift" => mods |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        let code = parse_key_code(key_token)?;
        Some(Self { mods, code })
    }
}

fn parse_key_code(s: &str) -> Option<KeyCode> {
        let lower = s.to_ascii_lowercase();
        Some(match lower.as_str() {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "space" => KeyCode::Char(' '),
            other => {
                if let Some(rest) = other.strip_prefix('f') {
                    if let Ok(n) = rest.parse::<u8>() {
                        if (1..=24).contains(&n) {
                            return Some(KeyCode::F(n));
                        }
                    }
                }
                let mut chars = other.chars();
                let first = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                // 文字キーは小文字に正規化して保持。Shift 修飾は別管理。
                KeyCode::Char(first)
            }
        })
}

// ─── KeyMap ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Pane,
    FileTree,
    Preview,
}

#[derive(Debug, Clone, Default)]
pub struct KeyMap {
    pub global: HashMap<KeyChord, Action>,
    pub pane: HashMap<KeyChord, Action>,
    pub file_tree: HashMap<KeyChord, Action>,
    pub preview: HashMap<KeyChord, Action>,
}

impl KeyMap {
    pub fn defaults() -> Self {
        use KeyCode::*;
        let ctrl = KeyModifiers::CONTROL;
        let alt = KeyModifiers::ALT;
        let none = KeyModifiers::NONE;

        let mut m = Self::default();

        // global
        let g = &mut m.global;
        g.insert(KeyChord::new(ctrl, Char('q')), Action::Quit);
        g.insert(KeyChord::new(ctrl, Char('t')), Action::NewTab);
        g.insert(KeyChord::new(alt, Char('t')), Action::NewTab);
        g.insert(KeyChord::new(alt, Right), Action::NextTab);
        g.insert(KeyChord::new(alt, Left), Action::PrevTab);
        g.insert(KeyChord::new(alt, Char('r')), Action::RenameTab);
        g.insert(KeyChord::new(alt, Char('s')), Action::ToggleStatusBar);
        g.insert(KeyChord::new(ctrl, Char('f')), Action::ToggleFileTree);
        g.insert(KeyChord::new(ctrl, Char('p')), Action::SwapLayout);
        g.insert(KeyChord::new(ctrl, Right), Action::FocusNextPane);
        g.insert(KeyChord::new(ctrl, Left), Action::FocusPrevPane);
        g.insert(KeyChord::new(ctrl, Char('c')), Action::CopySelectionOrPassthrough);

        // pane
        let p = &mut m.pane;
        p.insert(KeyChord::new(ctrl, Char('d')), Action::SplitVertical);
        p.insert(KeyChord::new(ctrl, Char('e')), Action::SplitHorizontal);
        p.insert(KeyChord::new(ctrl, Char('w')), Action::ClosePaneOrTab);
        p.insert(KeyChord::new(none, Esc), Action::ClearSelectionOrPassthrough);

        // file_tree
        let f = &mut m.file_tree;
        f.insert(KeyChord::new(none, Char('j')), Action::FileTreeDown);
        f.insert(KeyChord::new(none, Down), Action::FileTreeDown);
        f.insert(KeyChord::new(none, Char('k')), Action::FileTreeUp);
        f.insert(KeyChord::new(none, Up), Action::FileTreeUp);
        f.insert(KeyChord::new(none, Enter), Action::FileTreeOpen);
        f.insert(KeyChord::new(none, Char('.')), Action::FileTreeToggleHidden);
        f.insert(KeyChord::new(none, Esc), Action::FileTreeBlur);

        // preview
        let v = &mut m.preview;
        v.insert(KeyChord::new(none, Char('j')), Action::PreviewScrollDown);
        v.insert(KeyChord::new(none, Down), Action::PreviewScrollDown);
        v.insert(KeyChord::new(none, Char('k')), Action::PreviewScrollUp);
        v.insert(KeyChord::new(none, Up), Action::PreviewScrollUp);
        v.insert(KeyChord::new(none, PageDown), Action::PreviewPageDown);
        v.insert(KeyChord::new(none, PageUp), Action::PreviewPageUp);
        v.insert(KeyChord::new(none, Char('l')), Action::PreviewScrollRight);
        v.insert(KeyChord::new(none, Right), Action::PreviewScrollRight);
        v.insert(KeyChord::new(KeyModifiers::SHIFT, Right), Action::PreviewScrollRight);
        v.insert(KeyChord::new(none, Char('h')), Action::PreviewScrollLeft);
        v.insert(KeyChord::new(none, Left), Action::PreviewScrollLeft);
        v.insert(KeyChord::new(KeyModifiers::SHIFT, Left), Action::PreviewScrollLeft);
        v.insert(KeyChord::new(none, Home), Action::PreviewScrollHome);
        v.insert(KeyChord::new(ctrl, Char('w')), Action::PreviewClose);
        v.insert(KeyChord::new(none, Esc), Action::PreviewBlur);

        m
    }

    pub fn lookup(&self, scope: Scope, chord: KeyChord) -> Option<Action> {
        let map = match scope {
            Scope::Global => &self.global,
            Scope::Pane => &self.pane,
            Scope::FileTree => &self.file_tree,
            Scope::Preview => &self.preview,
        };
        map.get(&chord).copied()
    }

    /// ユーザ設定を既定値の上にマージする。
    /// - 同じキーは上書き
    /// - アクションが `"none"` のときはそのキーを削除 (無効化)
    pub fn apply_user(&mut self, user: &KeybindingsCfg) {
        merge(&mut self.global, &user.global);
        merge(&mut self.pane, &user.pane);
        merge(&mut self.file_tree, &user.file_tree);
        merge(&mut self.preview, &user.preview);
    }
}

fn merge(dst: &mut HashMap<KeyChord, Action>, src: &HashMap<KeyChord, Option<Action>>) {
    for (chord, act) in src {
        match act {
            Some(a) => {
                dst.insert(*chord, *a);
            }
            None => {
                dst.remove(chord);
            }
        }
    }
}

// ─── TOML deserialization ──────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KeybindingsCfg {
    #[serde(default, deserialize_with = "deser_scope")]
    pub global: HashMap<KeyChord, Option<Action>>,
    #[serde(default, deserialize_with = "deser_scope")]
    pub pane: HashMap<KeyChord, Option<Action>>,
    #[serde(default, deserialize_with = "deser_scope")]
    pub file_tree: HashMap<KeyChord, Option<Action>>,
    #[serde(default, deserialize_with = "deser_scope")]
    pub preview: HashMap<KeyChord, Option<Action>>,
}

fn deser_scope<'de, D>(de: D) -> Result<HashMap<KeyChord, Option<Action>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let raw: HashMap<String, String> = HashMap::deserialize(de)?;
    let mut out = HashMap::with_capacity(raw.len());
    for (k, v) in raw {
        let chord = KeyChord::parse(&k)
            .ok_or_else(|| D::Error::custom(format!("invalid key chord: {k:?}")))?;
        let action = if v.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(
                Action::parse(&v)
                    .ok_or_else(|| D::Error::custom(format!("unknown action: {v:?} for {k:?}")))?,
            )
        };
        out.insert(chord, action);
    }
    Ok(out)
}

// ─── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_chords() {
        assert_eq!(
            KeyChord::parse("ctrl+d"),
            Some(KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('d')))
        );
        assert_eq!(
            KeyChord::parse("Alt+Right"),
            Some(KeyChord::new(KeyModifiers::ALT, KeyCode::Right))
        );
        assert_eq!(
            KeyChord::parse("CTRL+SHIFT+f5"),
            Some(KeyChord::new(
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                KeyCode::F(5)
            ))
        );
        assert_eq!(
            KeyChord::parse("esc"),
            Some(KeyChord::new(KeyModifiers::NONE, KeyCode::Esc))
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(KeyChord::parse("").is_none());
        assert!(KeyChord::parse("ctrl+").is_none());
        assert!(KeyChord::parse("hyper+x").is_none());
        assert!(KeyChord::parse("ctrl+abc").is_none()); // 複数文字
    }

    #[test]
    fn defaults_have_core_bindings() {
        let m = KeyMap::defaults();
        assert_eq!(
            m.lookup(
                Scope::Global,
                KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('q'))
            ),
            Some(Action::Quit)
        );
        assert_eq!(
            m.lookup(
                Scope::Pane,
                KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('d'))
            ),
            Some(Action::SplitVertical)
        );
        assert_eq!(
            m.lookup(
                Scope::FileTree,
                KeyChord::new(KeyModifiers::NONE, KeyCode::Char('j'))
            ),
            Some(Action::FileTreeDown)
        );
    }

    #[test]
    fn user_config_overrides_default() {
        let toml_src = r#"
            [global]
            "ctrl+x" = "quit"
            "ctrl+q" = "none"

            [pane]
            "alt+v" = "split_vertical"
        "#;
        let cfg: KeybindingsCfg = toml::from_str(toml_src).unwrap();
        let mut m = KeyMap::defaults();
        m.apply_user(&cfg);

        // 新しい束縛が追加された
        assert_eq!(
            m.lookup(
                Scope::Global,
                KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('x'))
            ),
            Some(Action::Quit)
        );
        // "none" で既定が削除された
        assert_eq!(
            m.lookup(
                Scope::Global,
                KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('q'))
            ),
            None
        );
        // 別スコープ追加
        assert_eq!(
            m.lookup(
                Scope::Pane,
                KeyChord::new(KeyModifiers::ALT, KeyCode::Char('v'))
            ),
            Some(Action::SplitVertical)
        );
        // 触っていないものは既定のまま
        assert_eq!(
            m.lookup(
                Scope::Pane,
                KeyChord::new(KeyModifiers::CONTROL, KeyCode::Char('d'))
            ),
            Some(Action::SplitVertical)
        );
    }

    #[test]
    fn invalid_action_is_rejected() {
        let toml_src = r#"
            [global]
            "ctrl+x" = "fly_to_the_moon"
        "#;
        let r: Result<KeybindingsCfg, _> = toml::from_str(toml_src);
        assert!(r.is_err());
    }

    #[test]
    fn invalid_chord_is_rejected() {
        let toml_src = r#"
            [global]
            "hyper+x" = "quit"
        "#;
        let r: Result<KeybindingsCfg, _> = toml::from_str(toml_src);
        assert!(r.is_err());
    }
}
