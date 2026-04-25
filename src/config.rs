use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub scrollback: ScrollbackCfg,
    #[serde(default)]
    pub osc52: Osc52Cfg,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScrollbackCfg {
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Osc52Cfg {
    #[serde(default)]
    pub allow_read: bool,
}

fn default_max_lines() -> usize {
    10000
}


impl Default for ScrollbackCfg {
    fn default() -> Self {
        Self { max_lines: default_max_lines() }
    }
}

impl Config {
    pub fn load() -> Self {
        let Some(dir) = dirs::config_dir() else { return Self::default() };
        let path = dir.join("ccmux/config.toml");
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        toml::from_str(&text).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.scrollback.max_lines, 10000);
        assert!(!c.osc52.allow_read);
    }

    #[test]
    fn parses_partial_toml() {
        let s = r#"
            [scrollback]
            max_lines = 5000
        "#;
        let c: Config = toml::from_str(s).unwrap();
        assert_eq!(c.scrollback.max_lines, 5000);
        assert!(!c.osc52.allow_read);
    }

    #[test]
    fn parses_osc52_allow_read() {
        let s = r#"
            [osc52]
            allow_read = true
        "#;
        let c: Config = toml::from_str(s).unwrap();
        assert!(c.osc52.allow_read);
        assert_eq!(c.scrollback.max_lines, 10000);
    }

    #[test]
    fn empty_toml_uses_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.scrollback.max_lines, 10000);
        assert!(!c.osc52.allow_read);
    }
}
