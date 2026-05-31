use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::scm::ScmKind;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptProfile {
    pub architecture_summary: Option<String>,
    pub architecture_rules: Vec<String>,
    pub unchanged_code_guidance: Option<String>,
    pub review_focus: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub extra_instructions: Option<String>,
}

pub fn built_in_default_profile() -> PromptProfile {
    PromptProfile {
        architecture_summary: Some(
            "FrontEnd -> GraphQL API resolvers/mutations -> Service -> Repository -> DB via SQLAlchemy models"
                .to_string(),
        ),
        architecture_rules: vec![
            "resolvers/mutations call the Service layer only".to_string(),
            "resolvers/mutations do NOT call Repository classes directly".to_string(),
            "resolvers/mutations do NOT access SQLAlchemy models, DB sessions, or raw queries directly".to_string(),
            "Service layer owns business logic and orchestration".to_string(),
            "Repository layer owns persistence and DB access".to_string(),
            "dependencies flow downward only".to_string(),
            "no architectural layer is skipped".to_string(),
            "transaction/session handling remains consistent with existing patterns".to_string(),
            "If a blocking architectural issue is marked with `# ARCH-EXCEPTION: approved:<ticket>, since:<date>, reason:<text>`, do NOT flag it as a blocking issue. Only warn if the marker is malformed or missing required fields.".to_string(),
        ],
        unchanged_code_guidance: Some(
            "Assume unchanged code is out of scope unless required for understanding impact."
                .to_string(),
        ),
        review_focus: vec![
            "bugs and regressions".to_string(),
            "security issues".to_string(),
            "AWS/IAM/CDK/infrastructure risks".to_string(),
            "missing or weak tests".to_string(),
            "backward compatibility".to_string(),
            "maintainability".to_string(),
            "unclear, fragile, or overly complex design".to_string(),
            "risky deployment, migration, or rollback concerns".to_string(),
            "transaction/data consistency risks".to_string(),
            "authorization/authentication mistakes".to_string(),
            "concurrency, async, caching, or state-management risks".to_string(),
            "performance regressions caused by the change".to_string(),
        ],
        out_of_scope: vec![
            "the new changes introduce risk into that area".to_string(),
            "the modified code depends on fragile existing behavior".to_string(),
            "there is an obvious regression/security concern directly connected to the diff"
                .to_string(),
        ],
        extra_instructions: Some(
            "Flag any layering violation, dependency inversion, or bypassed abstraction.\n\nDo not rewrite the code yet.\nPrefer fewer high-confidence findings over many speculative comments.\nCall out only actionable findings."
                .to_string(),
        ),
    }
}

pub fn resolve_prompt_profile(
    scm_kind: Option<ScmKind>,
    repository: &str,
    repo_path: &Path,
) -> Result<PromptProfile> {
    let mut profile = built_in_default_profile();

    let mut candidates = vec![repo_path.join(".pr-review").join("prompt.toml")];

    if let Some(kind) = scm_kind {
        let prompts_root = user_prompts_dir()?;
        let repo_key = profile_repo_key(repository);
        candidates.push(
            prompts_root
                .join(kind.config_dir_name())
                .join(format!("{repo_key}.toml")),
        );
        candidates.push(prompts_root.join(kind.config_dir_name()).join("default.toml"));
    }

    for path in candidates {
        if path.exists() {
            let parsed = load_prompt_profile(&path)?;
            profile = profile.merge_override(parsed);
        }
    }

    Ok(profile)
}

pub enum PromptInitStatus {
    Created,
    Unchanged,
}

pub struct PromptInitResult {
    pub default_path: PathBuf,
    pub default_status: PromptInitStatus,
    pub repo_path: PathBuf,
    pub repo_status: PromptInitStatus,
}

pub fn init_user_prompt_profiles(
    scm_kind: ScmKind,
    repository: &str,
) -> Result<PromptInitResult> {
    let prompts_dir = user_prompts_dir()?.join(scm_kind.config_dir_name());
    let default_path = prompts_dir.join("default.toml");
    let repo_path = prompts_dir.join(format!("{}.toml", profile_repo_key(repository)));

    fs::create_dir_all(&prompts_dir).with_context(|| {
        format!(
            "Failed to create user prompt profile directory: {}",
            prompts_dir.display()
        )
    })?;

    let default_status = write_prompt_template_if_missing(&default_path)?;
    let repo_status = write_prompt_template_if_missing(&repo_path)?;

    Ok(PromptInitResult {
        default_path,
        default_status,
        repo_path,
        repo_status,
    })
}

fn load_prompt_profile(path: &Path) -> Result<PromptProfile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read prompt profile: {}", path.display()))?;
    parse_prompt_profile(&raw)
        .with_context(|| format!("Failed to parse prompt profile: {}", path.display()))
}

fn user_prompts_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;
    Ok(PathBuf::from(home).join(".pr-review").join("prompts"))
}

fn default_prompt_profile_template() -> &'static str {
    r#"# pr-review prompt profile

[architecture]
summary = "FrontEnd -> GraphQL API resolvers/mutations -> Service -> Repository -> DB via SQLAlchemy models"
rules = [
  "resolvers/mutations call the Service layer only",
  "resolvers/mutations do NOT call Repository classes directly",
  "resolvers/mutations do NOT access SQLAlchemy models, DB sessions, or raw queries directly",
  "Service layer owns business logic and orchestration",
  "Repository layer owns persistence and DB access",
  "dependencies flow downward only",
  "no architectural layer is skipped",
  "transaction/session handling remains consistent with existing patterns",
  "If a blocking architectural issue is marked with `# ARCH-EXCEPTION: approved:<ticket>, since:<date>, reason:<text>`, do NOT flag it as a blocking issue. Only warn if the marker is malformed or missing required fields."
]
unchanged_code_guidance = "Assume unchanged code is out of scope unless required for understanding impact."

[review]
focus = [
  "bugs and regressions",
  "security issues",
  "AWS/IAM/CDK/infrastructure risks",
  "missing or weak tests",
  "backward compatibility",
  "maintainability",
  "unclear, fragile, or overly complex design",
  "risky deployment, migration, or rollback concerns",
  "transaction/data consistency risks",
  "authorization/authentication mistakes",
  "concurrency, async, caching, or state-management risks",
  "performance regressions caused by the change"
]
out_of_scope = [
  "the new changes introduce risk into that area",
  "the modified code depends on fragile existing behavior",
  "there is an obvious regression/security concern directly connected to the diff"
]

[prompt]
extra_instructions = """
Flag any layering violation, dependency inversion, or bypassed abstraction.

Do not rewrite the code yet.
Prefer fewer high-confidence findings over many speculative comments.
Call out only actionable findings.
"""
"#
}

fn write_prompt_template_if_missing(path: &Path) -> Result<PromptInitStatus> {
    if path.exists() {
        return Ok(PromptInitStatus::Unchanged);
    }

    fs::write(path, default_prompt_profile_template())
        .with_context(|| format!("Failed to write prompt profile template: {}", path.display()))?;

    Ok(PromptInitStatus::Created)
}

impl PromptProfile {
    fn merge_override(mut self, override_profile: PromptProfile) -> Self {
        if override_profile.architecture_summary.is_some() {
            self.architecture_summary = override_profile.architecture_summary;
        }

        if !override_profile.architecture_rules.is_empty() {
            self.architecture_rules = override_profile.architecture_rules;
        }

        if override_profile.unchanged_code_guidance.is_some() {
            self.unchanged_code_guidance = override_profile.unchanged_code_guidance;
        }

        if !override_profile.review_focus.is_empty() {
            self.review_focus = override_profile.review_focus;
        }

        if !override_profile.out_of_scope.is_empty() {
            self.out_of_scope = override_profile.out_of_scope;
        }

        if override_profile.extra_instructions.is_some() {
            self.extra_instructions = override_profile.extra_instructions;
        }

        self
    }
}

fn parse_prompt_profile(raw: &str) -> Result<PromptProfile> {
    let mut profile = PromptProfile::default();
    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    let mut section = "";

    while i < lines.len() {
        let line = lines[i].trim();

        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_start_matches('[').trim_end_matches(']').trim();
            i += 1;
            continue;
        }

        let Some((key, rest)) = line.split_once('=') else {
            i += 1;
            continue;
        };

        let key = key.trim();
        let mut value = rest.trim().to_string();

        if value.starts_with('[') && !value.contains(']') {
            while !value.contains(']') && i + 1 < lines.len() {
                i += 1;
                value.push('\n');
                value.push_str(lines[i].trim());
            }
        } else if value.starts_with("\"\"\"") && !value[3..].contains("\"\"\"") {
            while !value[3..].contains("\"\"\"") && i + 1 < lines.len() {
                i += 1;
                value.push('\n');
                value.push_str(lines[i]);
            }
        }

        match (section, key) {
            ("architecture", "summary") => {
                profile.architecture_summary = parse_string_value(&value);
            }
            ("architecture", "rules") => {
                profile.architecture_rules = parse_string_array(&value);
            }
            ("architecture", "unchanged_code_guidance") => {
                profile.unchanged_code_guidance = parse_string_value(&value);
            }
            ("review", "focus") => {
                profile.review_focus = parse_string_array(&value);
            }
            ("review", "out_of_scope") => {
                profile.out_of_scope = parse_string_array(&value);
            }
            ("prompt", "extra_instructions") => {
                profile.extra_instructions = parse_string_value(&value);
            }
            _ => {}
        }

        i += 1;
    }

    Ok(profile)
}

fn parse_string_value(raw: &str) -> Option<String> {
    let value = raw.trim();

    if value.starts_with("\"\"\"") && value.ends_with("\"\"\"") && value.len() >= 6 {
        return Some(value[3..value.len() - 3].trim().to_string());
    }

    if ((value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\'')))
        && value.len() >= 2
    {
        return Some(value[1..value.len() - 1].to_string());
    }

    None
}

fn parse_string_array(raw: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut in_string = false;
    let mut quote = '\0';
    let mut current = String::new();

    for ch in raw.chars() {
        if !in_string {
            if ch == '"' || ch == '\'' {
                in_string = true;
                quote = ch;
                current.clear();
            }
            continue;
        }

        if ch == quote {
            items.push(current.clone());
            current.clear();
            in_string = false;
            quote = '\0';
        } else {
            current.push(ch);
        }
    }

    items
}

pub fn profile_repo_key(repository: &str) -> String {
    repository
        .chars()
        .map(|ch| match ch {
            '/' => "__".to_string(),
            ' ' => "_".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_profile_fields() {
        let profile = parse_prompt_profile(
            r#"
[architecture]
summary = "Frontend -> API -> Service -> Repo"
rules = ["API calls services only", "Services own business logic"]
unchanged_code_guidance = "Only inspect unchanged code if needed."

[review]
focus = ["bugs", "security"]
out_of_scope = ["formatting", "style nits"]

[prompt]
extra_instructions = """
Prefer concise findings.
Avoid speculation.
"""
"#,
        )
        .unwrap();

        assert_eq!(
            profile.architecture_summary.as_deref(),
            Some("Frontend -> API -> Service -> Repo")
        );
        assert_eq!(profile.architecture_rules.len(), 2);
        assert_eq!(profile.review_focus, vec!["bugs", "security"]);
        assert_eq!(profile.out_of_scope, vec!["formatting", "style nits"]);
        assert!(profile
            .extra_instructions
            .as_deref()
            .unwrap_or_default()
            .contains("Avoid speculation"));
    }

    #[test]
    fn repo_key_sanitizes_slashes() {
        assert_eq!(profile_repo_key("PLATFORM/api"), "PLATFORM__api");
    }

    #[test]
    fn override_profile_replaces_non_empty_fields() {
        let base = built_in_default_profile();
        let override_profile = PromptProfile {
            architecture_summary: Some("API -> Service -> DB".to_string()),
            architecture_rules: vec!["Services own orchestration".to_string()],
            unchanged_code_guidance: None,
            review_focus: vec!["bugs".to_string()],
            out_of_scope: vec!["formatting".to_string()],
            extra_instructions: Some("Keep it strict.".to_string()),
        };

        let merged = base.merge_override(override_profile);

        assert_eq!(
            merged.architecture_summary.as_deref(),
            Some("API -> Service -> DB")
        );
        assert_eq!(
            merged.architecture_rules,
            vec!["Services own orchestration".to_string()]
        );
        assert_eq!(merged.review_focus, vec!["bugs".to_string()]);
        assert_eq!(merged.out_of_scope, vec!["formatting".to_string()]);
        assert_eq!(merged.extra_instructions.as_deref(), Some("Keep it strict."));
    }
}
