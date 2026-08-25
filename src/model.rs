use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptMetadata {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub metadata: PromptMetadata,
    pub body: String,
    pub path: PathBuf,
}

impl Prompt {
    pub fn parse(contents: &str, path: PathBuf) -> Result<Self> {
        let normalized = contents.replace("\r\n", "\n");
        let remainder = normalized
            .strip_prefix("+++\n")
            .context("prompt must begin with a +++ TOML frontmatter delimiter")?;
        let delimiter = "\n+++\n";
        let end = remainder
            .find(delimiter)
            .context("prompt frontmatter is missing its closing +++ delimiter")?;
        let metadata_text = &remainder[..end];
        let body = remainder[end + delimiter.len()..].to_owned();
        let metadata: PromptMetadata =
            toml::from_str(metadata_text).context("invalid prompt frontmatter")?;

        let prompt = Self {
            metadata,
            body,
            path,
        };
        prompt.validate()?;
        Ok(prompt)
    }

    pub fn validate(&self) -> Result<()> {
        if self.metadata.title.trim().is_empty() {
            bail!("prompt title cannot be empty");
        }
        if self.body.trim().is_empty() {
            bail!("prompt body cannot be empty");
        }
        Ok(())
    }

    pub fn to_markdown(&self) -> Result<String> {
        let metadata = toml::to_string_pretty(&self.metadata)
            .context("could not serialize prompt frontmatter")?;
        Ok(format!("+++\n{metadata}+++\n{}", self.body))
    }

    pub fn new_draft(title: String, tags: Vec<String>, path: PathBuf) -> Self {
        Self {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title,
                tags,
                aliases: Vec::new(),
            },
            body: String::new(),
            path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "d91f5f9e-1820-4db3-8960-bbaf02850d3e";

    #[test]
    fn parses_toml_frontmatter_and_preserves_body() {
        let input = format!(
            "+++\nid = \"{ID}\"\ntitle = \"Code review\"\ntags = [\"rust\"]\naliases = [\"review\"]\n+++\nReview this {{{{language}}}} code.\n"
        );
        let prompt = Prompt::parse(&input, "review.md".into()).unwrap();

        assert_eq!(prompt.metadata.title, "Code review");
        assert_eq!(prompt.metadata.tags, vec!["rust"]);
        assert_eq!(prompt.body, "Review this {{language}} code.\n");
    }

    #[test]
    fn round_trip_keeps_metadata_and_body() {
        let original = Prompt {
            metadata: PromptMetadata {
                id: Uuid::parse_str(ID).unwrap(),
                title: "Standup".into(),
                tags: vec!["work".into(), "writing".into()],
                aliases: vec![],
            },
            body: "Write a standup for {{project}}.\n".into(),
            path: "standup.md".into(),
        };

        let encoded = original.to_markdown().unwrap();
        let decoded = Prompt::parse(&encoded, original.path.clone()).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn rejects_missing_frontmatter_and_empty_body() {
        assert!(Prompt::parse("hello", "bad.md".into()).is_err());
        let empty = format!("+++\nid = \"{ID}\"\ntitle = \"Empty\"\n+++\n");
        assert!(Prompt::parse(&empty, "empty.md".into()).is_err());
    }
}
