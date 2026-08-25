use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use crossterm::event;
use prompts_wallet::{
    app::{Action, App, Screen},
    clipboard::{Clipboard, SystemClipboard},
    editor::{EditOutcome, EditorCommand, create_prompt, edit_prompt},
    storage::{Config, Vault, WalletPaths},
    ui::InlineUi,
    usage::UsageStore,
};

#[derive(Debug, Parser)]
#[command(name = "pwt", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List all prompts without opening the interactive wallet.
    List,
    /// Print the best matching prompt, leaving variables untouched.
    Show { query: String },
    /// Copy the best matching prompt, leaving variables untouched.
    Copy { query: String },
    /// Create a prompt using the configured editor.
    New {
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Edit the best matching prompt using the configured editor.
    Edit { query: String },
    /// Print the resolved wallet paths.
    Paths,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = WalletPaths::discover()?;
    paths.ensure()?;
    let config = Config::load(&paths.config_file)?;
    let vault = Vault::load(&paths)?;
    let usage = UsageStore::load(&paths.usage_file)?;

    match cli.command {
        None => {
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                bail!("interactive mode requires a terminal; try `pwt --help`");
            }
            run_interactive(paths, config, vault, usage)
        }
        Some(Commands::List) => {
            for prompt in vault.prompts {
                println!("{}\t{}", prompt.metadata.id, prompt.metadata.title);
            }
            Ok(())
        }
        Some(Commands::Show { query }) => {
            let prompt = vault
                .best_match(&query, &usage)
                .with_context(|| format!("no prompt matched {query:?}"))?;
            print_text(&prompt.body);
            Ok(())
        }
        Some(Commands::Copy { query }) => {
            let prompt = vault
                .best_match(&query, &usage)
                .with_context(|| format!("no prompt matched {query:?}"))?;
            let mut clipboard = SystemClipboard;
            clipboard.set_text(&prompt.body)?;
            let mut usage = usage;
            usage.record_now(prompt.metadata.id)?;
            usage.save(&paths.usage_file)?;
            println!("✓ Copied “{}” to clipboard", prompt.metadata.title);
            Ok(())
        }
        Some(Commands::New { title, tags }) => {
            let title = match title {
                Some(title) => title,
                None => read_required_line("Title: ")?,
            };
            let editor = EditorCommand::resolve(&config);
            let outcome = create_prompt(&paths, &vault, &editor, title, tags)?;
            let EditOutcome::Created(prompt) = outcome else {
                unreachable!()
            };
            println!("✓ Added “{}”", prompt.metadata.title);
            Ok(())
        }
        Some(Commands::Edit { query }) => {
            let prompt = vault
                .best_match(&query, &usage)
                .with_context(|| format!("no prompt matched {query:?}"))?
                .clone();
            let editor = EditorCommand::resolve(&config);
            let outcome = edit_prompt(&paths, &vault, &editor, &prompt)?;
            let EditOutcome::Updated(prompt) = outcome else {
                unreachable!()
            };
            println!("✓ Updated “{}”", prompt.metadata.title);
            Ok(())
        }
        Some(Commands::Paths) => {
            println!("config\t{}", paths.config_file.display());
            println!("prompts\t{}", paths.prompts_dir.display());
            println!("drafts\t{}", paths.drafts_dir.display());
            println!("usage\t{}", paths.usage_file.display());
            Ok(())
        }
    }
}

fn run_interactive(
    paths: WalletPaths,
    config: Config,
    vault: Vault,
    usage: UsageStore,
) -> Result<()> {
    let preferred_height = config.viewport_lines.unwrap_or(10);
    let editor = EditorCommand::resolve(&config);
    let mut app = App::new(vault, usage, paths.clone());
    let mut clipboard = SystemClipboard;
    let mut ui = InlineUi::new(preferred_height)?;
    let mut printed: Option<(String, String)> = None;

    loop {
        ui.draw(&app)?;
        let action = app.handle_event(event::read()?);
        match action {
            Action::None => {}
            Action::Quit => break,
            Action::Resize => ui.cycle_height()?,
            Action::Edit(id) => {
                let Some(prompt) = app
                    .prompts
                    .iter()
                    .find(|prompt| prompt.metadata.id == id)
                    .cloned()
                else {
                    app.notice = Some("Prompt no longer exists".into());
                    continue;
                };
                let anchor = ui.suspend()?;
                let current_vault = Vault::load(&paths)?;
                let result = edit_prompt(&paths, &current_vault, &editor, &prompt);
                ui.resume(anchor)?;
                match result {
                    Ok(EditOutcome::Updated(updated)) => {
                        app.replace_vault(Vault::load(&paths)?, Some(updated.metadata.id));
                        app.notice = Some(format!("Updated “{}”", updated.metadata.title));
                        app.screen = Screen::Home;
                        app.exit_status = format!("✓ Updated “{}”", updated.metadata.title);
                    }
                    Ok(EditOutcome::Created(_)) => unreachable!(),
                    Err(error) => app.notice = Some(format!("Edit failed: {error:#}")),
                }
            }
            Action::NewPrompt { title, tags } => {
                let anchor = ui.suspend()?;
                let current_vault = Vault::load(&paths)?;
                let result = create_prompt(&paths, &current_vault, &editor, title, tags);
                ui.resume(anchor)?;
                match result {
                    Ok(EditOutcome::Created(created)) => {
                        app.replace_vault(Vault::load(&paths)?, Some(created.metadata.id));
                        app.notice = Some(format!("Added “{}”", created.metadata.title));
                        app.screen = Screen::Home;
                        app.exit_status = format!("✓ Added “{}”", created.metadata.title);
                    }
                    Ok(EditOutcome::Updated(_)) => unreachable!(),
                    Err(error) => app.notice = Some(format!("Draft not saved: {error:#}")),
                }
            }
            Action::Copy {
                id,
                title,
                text,
                exit,
            } => match clipboard.set_text(&text) {
                Ok(()) => {
                    app.record_successful_use(id, &title);
                    if let Err(error) = app.usage.save(&paths.usage_file) {
                        app.notice = Some(format!("Copied; usage save failed: {error:#}"));
                    }
                    if exit {
                        break;
                    }
                }
                Err(error) => app.notice = Some(format!("Clipboard failed: {error:#}")),
            },
            Action::Print { id, title, text } => {
                app.usage.record_now(id)?;
                app.usage.save(&paths.usage_file)?;
                app.exit_status = format!("✓ Printed “{title}”");
                printed = Some((text, title));
                break;
            }
        }
    }

    ui.finish()?;
    if let Some((text, title)) = printed {
        print_text(&text);
        println!("✓ Printed “{title}”");
    } else {
        println!("{}", app.exit_status);
    }
    Ok(())
}

fn read_required_line(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        bail!("value cannot be empty");
    }
    Ok(value)
}

fn print_text(text: &str) {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
}
