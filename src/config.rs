use anyhow::{Context, Result};
use std::{
    env, fs,
    path::PathBuf,
    sync::OnceLock,
};

use crate::cli::AiTool;
use crate::scm::ScmKind;

static USER_CONFIG: OnceLock<AppConfig> = OnceLock::new();

#[derive(Clone, Default)]
pub struct AppConfig {
    pub default_ai: Option<AiTool>,
    pub default_scm: Option<ScmKind>,
    pub copilot_icon: Option<String>,
    pub codex_icon: Option<String>,
    pub prompt_style: PromptStyle,
    pub bitbucket_url: Option<String>,
    pub bitbucket_project: Option<String>,
    pub bitbucket_repo: Option<String>,
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
    let mut in_scm_section = false;
    let mut in_bitbucket_section = false;

    for line in raw.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_ai_section = section == "ai";
            in_scm_section = section == "scm";
            in_bitbucket_section = section == "bitbucket";
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');

        if !in_ai_section
            && !in_scm_section
            && !in_bitbucket_section
            && key != "default_ai"
            && key != "default_tool"
            && key != "default_scm"
        {
            continue;
        }

        match key {
            "default_ai" | "default_tool" => {
                config.default_ai = Some(AiTool::from_config_value(value)?);
            }
            "default" | "default_scm" if in_scm_section => {
                config.default_scm = Some(ScmKind::from_config_value(value)?);
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
            "url" if in_bitbucket_section => {
                config.bitbucket_url = Some(value.to_string());
            }
            "project" if in_bitbucket_section => {
                config.bitbucket_project = Some(value.to_string());
            }
            "repo" if in_bitbucket_section => {
                config.bitbucket_repo = Some(value.to_string());
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

[scm]
# valid values: "codecommit" or "bitbucket"
default = "codecommit"

[bitbucket]
url = "https://bitbucket.example.com"
project = "MYPROJ"
repo = "my-repo"
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
