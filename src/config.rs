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
    pub copilot_model: Option<String>,
    pub copilot_sdk_model: Option<String>,
    pub codex_model: Option<String>,
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
    let mut in_copilot_section = false;
    let mut in_copilot_sdk_section = false;
    let mut in_codex_section = false;
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
            in_copilot_section = section == "copilot";
            in_copilot_sdk_section = section == "copilot_sdk";
            in_codex_section = section == "codex";
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
            "model" if in_copilot_section => {
                config.copilot_model = Some(value.to_string());
            }
            "model" if in_copilot_sdk_section => {
                config.copilot_sdk_model = Some(value.to_string());
            }
            "model" if in_codex_section => {
                config.codex_model = Some(value.to_string());
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

pub enum ConfigInitStatus {
    Created,
    Updated,
    Unchanged,
}

pub fn init_user_config() -> Result<(PathBuf, ConfigInitStatus)> {
    let path = config_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    if !path.exists() {
        fs::write(&path, default_config_template())
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        return Ok((path, ConfigInitStatus::Created));
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let updated = merge_missing_config_entries(&raw);

    if updated == raw {
        return Ok((path, ConfigInitStatus::Unchanged));
    }

    fs::write(&path, updated)
        .with_context(|| format!("Failed to update config file: {}", path.display()))?;

    Ok((path, ConfigInitStatus::Updated))
}

fn config_path() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;
    Ok(PathBuf::from(home).join(".pr-review").join("config.toml"))
}

fn default_config_template() -> &'static str {
    r#"# pr-review user configuration

[ai]
# valid values: "copilot", "codex", or "copilot-sdk" if built with that feature
default_ai = "codex"
# valid values: "fancy" or "simple"
prompt_style = "fancy"
copilot_icon = "🧑‍✈️"
codex_icon = "🤖"

[copilot]
model = "gpt-5"

[copilot_sdk]
model = "gpt-5"

[codex]
model = "gpt-5-codex"

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

fn merge_missing_config_entries(raw: &str) -> String {
    let mut lines: Vec<String> = raw.lines().map(ToString::to_string).collect();

    ensure_section(
        &mut lines,
        "ai",
        &[
            "# valid values: \"copilot\", \"codex\", or \"copilot-sdk\" if built with that feature",
            "default_ai = \"codex\"",
            "# valid values: \"fancy\" or \"simple\"",
            "prompt_style = \"fancy\"",
            "copilot_icon = \"🧑‍✈️\"",
            "codex_icon = \"🤖\"",
        ],
    );
    ensure_section(&mut lines, "copilot", &["model = \"gpt-5\""]);
    ensure_section(&mut lines, "copilot_sdk", &["model = \"gpt-5\""]);
    ensure_section(&mut lines, "codex", &["model = \"gpt-5-codex\""]);
    ensure_section(
        &mut lines,
        "scm",
        &[
            "# valid values: \"codecommit\" or \"bitbucket\"",
            "default = \"codecommit\"",
        ],
    );
    ensure_section(
        &mut lines,
        "bitbucket",
        &[
            "url = \"https://bitbucket.example.com\"",
            "project = \"MYPROJ\"",
            "repo = \"my-repo\"",
        ],
    );

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn ensure_section(lines: &mut Vec<String>, section: &str, entries: &[&str]) {
    let header = format!("[{section}]");
    let section_start = lines.iter().position(|line| line.trim() == header);

    match section_start {
        Some(start) => {
            let section_end = lines
                .iter()
                .enumerate()
                .skip(start + 1)
                .find(|(_, line)| line.trim().starts_with('[') && line.trim().ends_with(']'))
                .map(|(idx, _)| idx)
                .unwrap_or(lines.len());

            let mut missing = Vec::new();
            for entry in entries {
                if entry.trim_start().starts_with('#') {
                    continue;
                }

                let key = entry.split('=').next().unwrap_or("").trim();
                let has_key = lines[start + 1..section_end]
                    .iter()
                    .any(|line| line.split('=').next().map(|part| part.trim()) == Some(key));
                if !has_key {
                    missing.push((*entry).to_string());
                }
            }

            if !missing.is_empty() {
                let mut insert_at = section_end;
                if insert_at > start + 1 && !lines[insert_at - 1].trim().is_empty() {
                    lines.insert(insert_at, String::new());
                    insert_at += 1;
                }
                for entry in missing {
                    lines.insert(insert_at, entry);
                    insert_at += 1;
                }
            }
        }
        None => {
            if !lines.is_empty() && !lines.last().is_some_and(|line| line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.extend(entries.iter().map(|entry| (*entry).to_string()));
        }
    }
}
