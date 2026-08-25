use std::{
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};

pub trait Clipboard {
    fn set_text(&mut self, text: &str) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn set_text(&mut self, text: &str) -> Result<()> {
        if let Ok(mut clipboard) = arboard::Clipboard::new()
            && clipboard.set_text(text.to_owned()).is_ok()
        {
            return Ok(());
        }

        let mut attempts = Vec::new();
        if cfg!(target_os = "macos") {
            attempts.push(("pbcopy", vec![]));
        } else if is_wsl() {
            attempts.push(("clip.exe", vec![]));
        } else {
            attempts.push(("wl-copy", vec![]));
            attempts.push(("xclip", vec!["-selection", "clipboard"]));
        }

        let mut errors = Vec::new();
        for (program, arguments) in attempts {
            match pipe_to(program, &arguments, text) {
                Ok(()) => return Ok(()),
                Err(error) => errors.push(format!("{program}: {error}")),
            }
        }
        Err(anyhow!(
            "no clipboard backend succeeded: {}",
            errors.join("; ")
        ))
    }
}

fn pipe_to(program: &str, arguments: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not start {program}"))?;
    child
        .stdin
        .take()
        .context("clipboard process did not expose stdin")?
        .write_all(text.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::fs::read_to_string("/proc/version")
            .is_ok_and(|version| version.to_lowercase().contains("microsoft"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeClipboard {
        copied: Option<String>,
    }

    impl Clipboard for FakeClipboard {
        fn set_text(&mut self, text: &str) -> Result<()> {
            self.copied = Some(text.into());
            Ok(())
        }
    }

    #[test]
    fn clipboard_interface_is_testable_without_touching_the_system_clipboard() {
        let mut clipboard = FakeClipboard::default();
        clipboard.set_text("hello\nworld").unwrap();
        assert_eq!(clipboard.copied.as_deref(), Some("hello\nworld"));
    }
}
