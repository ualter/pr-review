use anyhow::{Context, Result};
use std::{
    env, fs,
    path::PathBuf,
    sync::OnceLock,
};

use crate::cli::AiTool;

static USER_CONFIG: OnceLock<AppConfig> = OnceLock::new();

#[derive(Clone, Default)]
pub struct AppConfig {
    pub default_ai: Option<AiTool>,
    pub copilot_icon: Option<String>,
    pub codex_icon: Option<String>,
    pub prompt_style: PromptStyle,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub enum PromptStyle {
    Simple,
    #[default]
    Fancy,
}

pub fn load_user_config() -> Result<AppConfig> {
    let path = config_path()?;

    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let mut config = AppConfig::default();
    let mut in_ai_section = false;

    for line in raw.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_ai_section = section == "ai";
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');

        if !in_ai_section && key != "default_ai" && key != "default_tool" {
            continue;
        }

        match key {
            "default_ai" | "default_tool" => {
                config.default_ai = Some(AiTool::from_config_value(value)?);
            }
            "copilot_icon" if in_ai_section => {
                config.copilot_icon = Some(value.to_string());
            }
            "codex_icon" if in_ai_section => {
                config.codex_icon = Some(value.to_string());
            }
            "prompt_style" if in_ai_section => {
                config.prompt_style = PromptStyle::from_config_value(value)?;
            }
            _ => {}
        }
    }

    Ok(config)
}

pub fn set_user_config(config: AppConfig) {
    let _ = USER_CONFIG.set(config);
}

pub fn user_config() -> &'static AppConfig {
    USER_CONFIG.get_or_init(AppConfig::default)
}

pub fn init_user_config() -> Result<(PathBuf, bool)> {
    let path = config_path()?;

    if path.exists() {
        return Ok((path, false));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    fs::write(&path, default_config_template())
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    Ok((path, true))
}

fn config_path() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;
    Ok(PathBuf::from(home).join(".pr-review").join("config.toml"))
}

fn default_config_template() -> &'static str {
    r#"# pr-review user configuration

[ai]
# valid values: "copilot" or "codex"
default_ai = "codex"
# valid values: "fancy" or "simple"
prompt_style = "fancy"
copilot_icon = "🧑‍✈️"
codex_icon = "🤖"
"#
}

impl PromptStyle {
    fn from_config_value(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "fancy" => Ok(PromptStyle::Fancy),
            "simple" => Ok(PromptStyle::Simple),
            other => Err(anyhow::anyhow!(
                "Invalid prompt style `{other}` in config. Expected `fancy` or `simple`."
            )),
        }
    }
}
