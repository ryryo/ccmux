//! UI のカラースキーム定義。
//!
//! `Theme` 構造体に UI 全域で使う色をまとめ、`Theme::light()` /
//! `Theme::dark()` の 2 バリアントを提供する。`config.toml` の
//! `[theme] mode = "light" | "dark"` で切り替える。

use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// 全体の背景色。
    pub bg: Color,
    /// パネル（ファイルツリー / プレビュー / ペイン本体）の背景色。
    pub panel_bg: Color,
    /// 通常時のボーダー色。
    pub border: Color,
    /// フォーカス時のボーダー色。
    pub focus_border: Color,
    /// 通常テキスト色。
    pub text: Color,
    /// 補助テキスト（アイコン default、ヒント等）の色。
    pub text_dim: Color,
    /// 強調（成功・OK 系）。
    pub accent_green: Color,
    /// 強調（リンク・情報系）。
    pub accent_blue: Color,
    /// Claude 起動中ペインのボーダー / アクセント。
    pub accent_claude: Color,
    /// タブバー / ステータスバー等のヘッダ背景。
    pub header_bg: Color,
    /// アクティブタブの背景。
    pub active_tab_bg: Color,
    /// 選択行（ファイルツリー等）の背景。
    pub active_bg: Color,
    /// プレビューの行番号色。
    pub line_num: Color,
    /// スクロールバック表示時の警告背景。
    pub scroll_bg: Color,
    /// syntect が使うテーマ名。`InspiredGitHub` (light) / `base16-eighties.dark` (dark) など。
    pub syntect_theme: &'static str,
}

impl Theme {
    /// GitHub Light を参考にしたライトテーマ（旧デフォルト）。
    pub const fn light() -> Self {
        Self {
            bg: Color::Rgb(0xf6, 0xf8, 0xfa),
            panel_bg: Color::Rgb(0xff, 0xff, 0xff),
            border: Color::Rgb(0xd0, 0xd7, 0xde),
            focus_border: Color::Rgb(0x01, 0x69, 0xda),
            text: Color::Rgb(0x1f, 0x23, 0x28),
            text_dim: Color::Rgb(0x65, 0x6d, 0x76),
            accent_green: Color::Rgb(0x1a, 0x7f, 0x37),
            accent_blue: Color::Rgb(0x01, 0x69, 0xda),
            accent_claude: Color::Rgb(0xc9, 0x5f, 0x2e),
            header_bg: Color::Rgb(0xef, 0xf2, 0xf5),
            active_tab_bg: Color::Rgb(0xf6, 0xf8, 0xfa),
            active_bg: Color::Rgb(0xe8, 0xf0, 0xfe),
            line_num: Color::Rgb(0xaf, 0xb8, 0xc1),
            scroll_bg: Color::Rgb(0xf5, 0xe6, 0xd8),
            syntect_theme: "InspiredGitHub",
        }
    }

    /// GitHub Dark を参考にしたダークテーマ。
    pub const fn dark() -> Self {
        Self {
            bg: Color::Rgb(0x0d, 0x11, 0x17),
            panel_bg: Color::Rgb(0x16, 0x1b, 0x22),
            border: Color::Rgb(0x30, 0x36, 0x3d),
            focus_border: Color::Rgb(0x58, 0xa6, 0xff),
            text: Color::Rgb(0xc9, 0xd1, 0xd9),
            text_dim: Color::Rgb(0x8b, 0x94, 0x9e),
            accent_green: Color::Rgb(0x3f, 0xb9, 0x50),
            accent_blue: Color::Rgb(0x58, 0xa6, 0xff),
            accent_claude: Color::Rgb(0xff, 0x8c, 0x42),
            header_bg: Color::Rgb(0x1c, 0x21, 0x28),
            active_tab_bg: Color::Rgb(0x0d, 0x11, 0x17),
            active_bg: Color::Rgb(0x1d, 0x29, 0x42),
            line_num: Color::Rgb(0x48, 0x4f, 0x58),
            scroll_bg: Color::Rgb(0x33, 0x2a, 0x1e),
            syntect_theme: "base16-eighties.dark",
        }
    }

    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Light
    }
}

impl<'de> serde::Deserialize<'de> for ThemeMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.to_ascii_lowercase().as_str() {
            "light" => Ok(ThemeMode::Light),
            "dark" => Ok(ThemeMode::Dark),
            other => Err(serde::de::Error::custom(format!(
                "unknown theme mode: {other:?} (expected \"light\" or \"dark\")"
            ))),
        }
    }
}
