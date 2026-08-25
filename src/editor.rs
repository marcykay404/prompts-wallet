use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
#[cfg(test)]
use uuid::Uuid;

use crate::{
    model::Prompt,
    storage::{Config, Vault, WalletPaths, atomic_write, ensure_unique_id, prompt_path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl EditorCommand {
    pub fn resolve(config: &Config) -> Self {
        Self::resolve_from(
            config.editor.clone(),
            env::var("VISUAL").ok(),
            env::var("EDITOR").ok(),
        )
    }

    fn resolve_from(
        configured: Option<Vec<String>>,
        visual: Option<String>,
        editor: Option<String>,
    ) -> Self {
        if let Some(command) = configured.filter(|command| !command.is_empty()) {
            return Self {
                program: command[0].clone(),
                args: command[1..].to_vec(),
            };
        }
        for value in [visual, editor].into_iter().flatten() {
            let mut parts = value.split_whitespace();
            if let Some(program) = parts.next() {
                return Self {
                    program: program.into(),
                    args: parts.map(str::to_owned).collect(),
                };
            }
        }
        Self {
            program: "vi".into(),
            args: vec![],
        }
    }
}

pub trait PromptEditor {
    fn edit(&self, path: &Path) -> Result<()>;
}

impl PromptEditor for EditorCommand {
    fn edit(&self, path: &Path) -> Result<()> {
        let status = Command::new(&self.program)
            .args(&self.args)
            .arg(path)
            .status()
            .with_context(|| format!("could not launch editor {}", self.program))?;
        if !status.success() {
            bail!(
                "editor exited with status {status}; draft kept at {}",
                path.display()
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    Created(Prompt),
    Updated(Prompt),
}

pub fn create_prompt<E: PromptEditor>(
    paths: &WalletPaths,
    vault: &Vault,
    editor: &E,
    title: String,
    tags: Vec<String>,
) -> Result<EditOutcome> {
    paths.ensure()?;
    let mut draft = Prompt::new_draft(title, tags, PathBuf::new());
    let id = draft.metadata.id;
    let draft_path = paths.drafts_dir.join(format!("new-{id}.md"));
    draft.path = draft_path.clone();
    atomic_write(&draft_path, draft.to_markdown()?.as_bytes())?;

    editor.edit(&draft_path)?;
    let contents = fs::read_to_string(&draft_path)
        .with_context(|| format!("could not read draft {}", draft_path.display()))?;
    let parsed = Prompt::parse(&contents, draft_path.clone())
        .with_context(|| format!("invalid draft kept at {}", draft_path.display()))?;
    if parsed.metadata.id != id {
        bail!(
            "prompt ID cannot be changed; draft kept at {}",
            draft_path.display()
        );
    }
    ensure_unique_id(vault, &parsed, None)?;
    let destination = prompt_path(paths, &parsed);
    atomic_write(&destination, contents.as_bytes())?;
    fs::remove_file(&draft_path)?;
    Ok(EditOutcome::Created(Prompt::parse(&contents, destination)?))
}

pub fn edit_prompt<E: PromptEditor>(
    paths: &WalletPaths,
    vault: &Vault,
    editor: &E,
    prompt: &Prompt,
) -> Result<EditOutcome> {
    paths.ensure()?;
    let draft_path = paths
        .drafts_dir
        .join(format!("edit-{}.md", prompt.metadata.id));
    let original = fs::read(&prompt.path)
        .with_context(|| format!("could not read {}", prompt.path.display()))?;
    atomic_write(&draft_path, &original)?;

    editor.edit(&draft_path)?;
    let contents = fs::read_to_string(&draft_path)
        .with_context(|| format!("could not read draft {}", draft_path.display()))?;
    let parsed = Prompt::parse(&contents, draft_path.clone())
        .with_context(|| format!("invalid draft kept at {}", draft_path.display()))?;
    if parsed.metadata.id != prompt.metadata.id {
        bail!(
            "prompt ID cannot be changed; draft kept at {}",
            draft_path.display()
        );
    }
    ensure_unique_id(vault, &parsed, Some(&prompt.path))?;
    atomic_write(&prompt.path, contents.as_bytes())?;
    fs::remove_file(&draft_path)?;
    Ok(EditOutcome::Updated(Prompt::parse(
        &contents,
        prompt.path.clone(),
    )?))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::*;
    use crate::model::PromptMetadata;

    struct FakeEditor {
        replacement: RefCell<Option<String>>,
    }

    impl PromptEditor for FakeEditor {
        fn edit(&self, path: &Path) -> Result<()> {
            if let Some(replacement) = self.replacement.borrow_mut().take() {
                fs::write(path, replacement)?;
            }
            Ok(())
        }
    }

    #[test]
    fn configured_editor_and_arguments_take_precedence() {
        let resolved = EditorCommand::resolve_from(
            Some(vec!["code".into(), "--wait".into()]),
            Some("hx".into()),
            Some("vim".into()),
        );
        assert_eq!(
            resolved,
            EditorCommand {
                program: "code".into(),
                args: vec!["--wait".into()]
            }
        );
    }

    #[test]
    fn creating_a_prompt_validates_then_moves_the_draft_into_the_vault() {
        let directory = tempdir().unwrap();
        let paths = WalletPaths::from_root(directory.path());
        paths.ensure().unwrap();
        let vault = Vault::load(&paths).unwrap();
        struct BodyEditor;
        impl PromptEditor for BodyEditor {
            fn edit(&self, path: &Path) -> Result<()> {
                let mut contents = fs::read_to_string(path)?;
                contents.push_str("Write a concise update for {{project}}.\n");
                fs::write(path, contents)?;
                Ok(())
            }
        }

        let outcome = create_prompt(
            &paths,
            &vault,
            &BodyEditor,
            "Standup".into(),
            vec!["work".into()],
        )
        .unwrap();

        let EditOutcome::Created(created) = outcome else {
            panic!("expected created outcome")
        };
        assert!(created.path.exists());
        assert_eq!(created.metadata.title, "Standup");
        assert!(fs::read_dir(&paths.drafts_dir).unwrap().next().is_none());
    }

    #[test]
    fn invalid_edit_never_overwrites_the_original_and_keeps_the_draft() {
        let directory = tempdir().unwrap();
        let paths = WalletPaths::from_root(directory.path());
        paths.ensure().unwrap();
        let prompt = Prompt {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title: "Original".into(),
                tags: vec![],
                aliases: vec![],
            },
            body: "Original body.\n".into(),
            path: paths.prompts_dir.join("original.md"),
        };
        let original = prompt.to_markdown().unwrap();
        fs::write(&prompt.path, &original).unwrap();
        let vault = Vault::load(&paths).unwrap();
        let editor = FakeEditor {
            replacement: RefCell::new(Some("not a valid prompt".into())),
        };

        let result = edit_prompt(&paths, &vault, &editor, &prompt);

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&prompt.path).unwrap(), original);
        assert!(
            paths
                .drafts_dir
                .join(format!("edit-{}.md", prompt.metadata.id))
                .exists()
        );
    }

    #[test]
    fn valid_edit_preserves_identity_replaces_content_and_removes_the_draft() {
        let directory = tempdir().unwrap();
        let paths = WalletPaths::from_root(directory.path());
        paths.ensure().unwrap();
        let prompt = Prompt {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title: "Original".into(),
                tags: vec![],
                aliases: vec![],
            },
            body: "Original body.\n".into(),
            path: paths.prompts_dir.join("original.md"),
        };
        fs::write(&prompt.path, prompt.to_markdown().unwrap()).unwrap();
        let vault = Vault::load(&paths).unwrap();
        struct ReplaceBody;
        impl PromptEditor for ReplaceBody {
            fn edit(&self, path: &Path) -> Result<()> {
                let contents = fs::read_to_string(path)?.replace("Original body.", "Updated body.");
                fs::write(path, contents)?;
                Ok(())
            }
        }

        let outcome = edit_prompt(&paths, &vault, &ReplaceBody, &prompt).unwrap();

        let EditOutcome::Updated(updated) = outcome else {
            panic!("expected updated outcome")
        };
        assert_eq!(updated.metadata.id, prompt.metadata.id);
        assert_eq!(updated.body, "Updated body.\n");
        assert!(
            !paths
                .drafts_dir
                .join(format!("edit-{}.md", prompt.metadata.id))
                .exists()
        );
    }
}
