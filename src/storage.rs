use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::model::Prompt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletPaths {
    pub config_file: PathBuf,
    pub prompts_dir: PathBuf,
    pub drafts_dir: PathBuf,
    pub usage_file: PathBuf,
}

impl WalletPaths {
    pub fn discover() -> Result<Self> {
        if let Some(root) = env::var_os("PW_HOME") {
            return Ok(Self::from_root(root));
        }

        let base = BaseDirs::new().context("could not determine platform data directories")?;
        let config_dir = base.config_dir().join("prompt-wallet");
        let data_dir = base.data_local_dir().join("prompt-wallet");
        let state_dir = base
            .state_dir()
            .unwrap_or_else(|| base.data_local_dir())
            .join("prompt-wallet");
        Ok(Self {
            config_file: config_dir.join("config.toml"),
            prompts_dir: data_dir.join("prompts"),
            drafts_dir: data_dir.join("drafts"),
            usage_file: state_dir.join("usage.json"),
        })
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_file: root.join("config.toml"),
            prompts_dir: root.join("prompts"),
            drafts_dir: root.join("drafts"),
            usage_file: root.join("usage.json"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.prompts_dir)
            .with_context(|| format!("could not create {}", self.prompts_dir.display()))?;
        fs::create_dir_all(&self.drafts_dir)
            .with_context(|| format!("could not create {}", self.drafts_dir.display()))?;
        if let Some(parent) = self.config_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.usage_file.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub editor: Option<Vec<String>>,
    pub viewport_lines: Option<u16>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents)
                .with_context(|| format!("invalid config file: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Vault {
    pub prompts: Vec<Prompt>,
    pub warnings: Vec<String>,
}

impl Vault {
    pub fn load(paths: &WalletPaths) -> Result<Self> {
        paths.ensure()?;
        let mut vault = Self::default();
        for entry in fs::read_dir(&paths.prompts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            match fs::read_to_string(&path)
                .with_context(|| format!("could not read {}", path.display()))
                .and_then(|contents| Prompt::parse(&contents, path.clone()))
            {
                Ok(prompt) => vault.prompts.push(prompt),
                Err(error) => vault
                    .warnings
                    .push(format!("{}: {error:#}", path.display())),
            }
        }
        vault
            .prompts
            .sort_by_key(|prompt| prompt.metadata.title.to_lowercase());
        Ok(vault)
    }

    pub fn by_id(&self, id: uuid::Uuid) -> Option<&Prompt> {
        self.prompts.iter().find(|prompt| prompt.metadata.id == id)
    }

    pub fn best_match(&self, query: &str, usage: &crate::usage::UsageStore) -> Option<&Prompt> {
        crate::search::search(&self.prompts, usage, query)
            .first()
            .map(|hit| &self.prompts[hit.index])
    }
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "prompt".into()
    } else {
        slug
    }
}

pub fn prompt_path(paths: &WalletPaths, prompt: &Prompt) -> PathBuf {
    let id = prompt.metadata.id.simple().to_string();
    paths.prompts_dir.join(format!(
        "{}-{}.md",
        slugify(&prompt.metadata.title),
        &id[..8]
    ))
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("target filename is not valid UTF-8")?;
    let temporary = parent.join(format!(".{filename}.tmp-{}", std::process::id()));
    {
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("could not create {}", temporary.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("could not replace {}", path.display()));
    }
    Ok(())
}

pub fn parse_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn ensure_unique_id(vault: &Vault, prompt: &Prompt, allowed_path: Option<&Path>) -> Result<()> {
    if vault.prompts.iter().any(|existing| {
        existing.metadata.id == prompt.metadata.id
            && allowed_path.is_none_or(|path| existing.path != path)
    }) {
        bail!("prompt ID {} is already in use", prompt.metadata.id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn portable_root_keeps_all_wallet_files_together() {
        let paths = WalletPaths::from_root("/tmp/example-wallet");
        assert_eq!(
            paths.config_file,
            PathBuf::from("/tmp/example-wallet/config.toml")
        );
        assert_eq!(
            paths.prompts_dir,
            PathBuf::from("/tmp/example-wallet/prompts")
        );
        assert_eq!(
            paths.usage_file,
            PathBuf::from("/tmp/example-wallet/usage.json")
        );
    }

    #[test]
    fn slugify_is_stable_and_files_include_an_id_suffix() {
        assert_eq!(
            slugify("  Security & Code Review!  "),
            "security-code-review"
        );
        assert_eq!(slugify("你好"), "prompt");
    }

    #[test]
    fn vault_keeps_valid_prompts_and_reports_invalid_files() {
        let directory = tempdir().unwrap();
        let paths = WalletPaths::from_root(directory.path());
        paths.ensure().unwrap();
        let valid = Prompt {
            metadata: crate::model::PromptMetadata {
                id: uuid::Uuid::new_v4(),
                title: "Valid".into(),
                tags: vec![],
                aliases: vec![],
            },
            body: "A useful body.\n".into(),
            path: PathBuf::new(),
        };
        fs::write(
            paths.prompts_dir.join("valid.md"),
            valid.to_markdown().unwrap(),
        )
        .unwrap();
        fs::write(paths.prompts_dir.join("broken.md"), "broken").unwrap();

        let vault = Vault::load(&paths).unwrap();

        assert_eq!(vault.prompts.len(), 1);
        assert_eq!(vault.warnings.len(), 1);
        assert!(vault.warnings[0].contains("broken.md"));
    }
}
