use std::collections::BTreeMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use crate::{
    model::Prompt,
    search::{SearchHit, frequent_indices, search},
    storage::{Vault, WalletPaths, parse_tags},
    template,
    usage::UsageStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    Variables,
    Preview,
    NewPrompt,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewField {
    Title,
    Tags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Resize,
    Edit(Uuid),
    NewPrompt {
        title: String,
        tags: Vec<String>,
    },
    Copy {
        id: Uuid,
        title: String,
        text: String,
        exit: bool,
    },
    Print {
        id: Uuid,
        title: String,
        text: String,
    },
}

#[derive(Debug, Clone)]
pub struct App {
    pub prompts: Vec<Prompt>,
    pub usage: UsageStore,
    pub paths: WalletPaths,
    pub screen: Screen,
    pub home_selected: usize,
    pub search_query: String,
    pub search_hits: Vec<SearchHit>,
    pub search_selected: usize,
    pub selected_id: Option<Uuid>,
    pub variable_names: Vec<String>,
    pub variable_values: BTreeMap<String, String>,
    pub variable_index: usize,
    pub variable_input: String,
    pub preview_scroll: u16,
    pub new_title: String,
    pub new_tags: String,
    pub new_field: NewField,
    pub help_scroll: u16,
    pub notice: Option<String>,
    pub exit_status: String,
    return_to_search: bool,
    help_return: Screen,
}

impl App {
    pub fn new(vault: Vault, usage: UsageStore, paths: WalletPaths) -> Self {
        let notice = (!vault.warnings.is_empty())
            .then(|| format!("Skipped {} invalid prompt file(s)", vault.warnings.len()));
        Self {
            prompts: vault.prompts,
            usage,
            paths,
            screen: Screen::Home,
            home_selected: 0,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_selected: 0,
            selected_id: None,
            variable_names: Vec::new(),
            variable_values: BTreeMap::new(),
            variable_index: 0,
            variable_input: String::new(),
            preview_scroll: 0,
            new_title: String::new(),
            new_tags: String::new(),
            new_field: NewField::Title,
            help_scroll: 0,
            notice,
            exit_status: "No prompt copied".into(),
            return_to_search: false,
            help_return: Screen::Home,
        }
    }

    pub fn home_indices(&self) -> Vec<usize> {
        frequent_indices(&self.prompts, &self.usage, 5)
    }

    pub fn selected_prompt(&self) -> Option<&Prompt> {
        let id = self.selected_id?;
        self.prompts.iter().find(|prompt| prompt.metadata.id == id)
    }

    pub fn rendered_prompt(&self) -> Option<String> {
        self.selected_prompt()
            .map(|prompt| template::render(&prompt.body, &self.variable_values))
    }

    pub fn help_text(&self) -> String {
        let lines = [
            "Prompt Wallet keeps prompts as markdown files with TOML frontmatter, then helps \
you find, fill in, and copy them fast."
                .to_string(),
            String::new(),
            "Home       Your most-used prompts. Enter/1-5 opens one, n creates a new prompt, \
e edits the selected one, s searches everything."
                .to_string(),
            "Search     Type to fuzzy-search every prompt by title, tag, or alias.".to_string(),
            "Variables  Prompts can hold {{name}} placeholders; fill each one before it's \
rendered."
                .to_string(),
            "Preview    Shows the rendered prompt. c copies, C copies and exits, p prints to \
stdout, e edits."
                .to_string(),
            String::new(),
            "Files".to_string(),
            format!("  config   {}", self.paths.config_file.display()),
            format!("  prompts  {}", self.paths.prompts_dir.display()),
            format!("  drafts   {}", self.paths.drafts_dir.display()),
            format!("  usage    {}", self.paths.usage_file.display()),
            String::new(),
            "Esc, q, or ? closes this screen.".to_string(),
        ];
        lines.join("\n")
    }

    pub fn replace_vault(&mut self, vault: Vault, preferred_id: Option<Uuid>) {
        self.prompts = vault.prompts;
        self.selected_id = preferred_id;
        self.search_hits = search(&self.prompts, &self.usage, &self.search_query);
        self.home_selected = 0;
        self.search_selected = 0;
    }

    pub fn record_successful_use(&mut self, id: Uuid, title: &str) {
        if let Err(error) = self.usage.record_now(id) {
            self.notice = Some(format!("Copied, but could not update usage: {error}"));
        } else {
            self.notice = Some(format!("Copied “{title}”"));
        }
        self.exit_status = format!("✓ Copied “{title}” to clipboard");
        self.screen = Screen::Home;
        self.home_selected = 0;
    }

    pub fn handle_event(&mut self, event: Event) -> Action {
        match event {
            Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                self.handle_key(key)
            }
            Event::Paste(text) => {
                match self.screen {
                    Screen::Search => self.search_query.push_str(&text),
                    Screen::Variables => self.variable_input.push_str(&text),
                    Screen::NewPrompt => match self.new_field {
                        NewField::Title => self.new_title.push_str(&text),
                        NewField::Tags => self.new_tags.push_str(&text),
                    },
                    _ => return Action::None,
                }
                self.refresh_search();
                Action::None
            }
            _ => Action::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.exit_status = "Cancelled".into();
            return Action::Quit;
        }
        if key.code == KeyCode::Char('v') && key.modifiers.contains(KeyModifiers::ALT) {
            return Action::Resize;
        }

        match self.screen {
            Screen::Home => self.handle_home(key),
            Screen::Search => self.handle_search(key),
            Screen::Variables => self.handle_variables(key),
            Screen::Preview => self.handle_preview(key),
            Screen::NewPrompt => self.handle_new_prompt(key),
            Screen::Help => self.handle_help(key),
        }
    }

    fn handle_home(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('v') => Action::Resize,
            KeyCode::Char('?') => {
                self.open_help();
                Action::None
            }
            KeyCode::Char('s') => {
                self.screen = Screen::Search;
                self.search_query.clear();
                self.refresh_search();
                Action::None
            }
            KeyCode::Char('n') => {
                self.new_title.clear();
                self.new_tags.clear();
                self.new_field = NewField::Title;
                self.screen = Screen::NewPrompt;
                Action::None
            }
            KeyCode::Char('e') => self
                .home_indices()
                .get(self.home_selected)
                .map(|index| Action::Edit(self.prompts[*index].metadata.id))
                .unwrap_or(Action::None),
            KeyCode::Char(character @ '1'..='5') => {
                let position = character.to_digit(10).unwrap() as usize - 1;
                if let Some(index) = self.home_indices().get(position).copied() {
                    self.open_prompt(index, false);
                }
                Action::None
            }
            KeyCode::Enter => {
                if let Some(index) = self.home_indices().get(self.home_selected).copied() {
                    self.open_prompt(index, false);
                }
                Action::None
            }
            KeyCode::Up => {
                self.home_selected = self.home_selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                let last = self.home_indices().len().saturating_sub(1);
                self.home_selected = (self.home_selected + 1).min(last);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_search(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('e') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return self
                .search_hits
                .get(self.search_selected)
                .map(|hit| Action::Edit(self.prompts[hit.index].metadata.id))
                .unwrap_or(Action::None);
        }
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Home;
                Action::None
            }
            KeyCode::Enter => {
                if let Some(hit) = self.search_hits.get(self.search_selected).copied() {
                    self.open_prompt(hit.index, true);
                }
                Action::None
            }
            KeyCode::Up => {
                self.search_selected = self.search_selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                let last = self.search_hits.len().saturating_sub(1);
                self.search_selected = (self.search_selected + 1).min(last);
                Action::None
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.refresh_search();
                Action::None
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_query.push(character);
                self.refresh_search();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_variables(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.return_to_list();
                Action::None
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.commit_variable();
                Action::None
            }
            KeyCode::BackTab => {
                self.store_current_variable();
                self.variable_index = self.variable_index.saturating_sub(1);
                self.load_current_variable();
                Action::None
            }
            KeyCode::Backspace => {
                self.variable_input.pop();
                Action::None
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.store_current_variable();
                self.screen = Screen::Preview;
                Action::None
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.variable_input.push(character);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_preview(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('v') => Action::Resize,
            KeyCode::Char('?') => {
                self.open_help();
                Action::None
            }
            KeyCode::Char('b') | KeyCode::Esc => {
                if self.variable_names.is_empty() {
                    self.return_to_list();
                } else {
                    self.screen = Screen::Variables;
                    self.load_current_variable();
                }
                Action::None
            }
            KeyCode::Char('e') => self.selected_id.map(Action::Edit).unwrap_or(Action::None),
            KeyCode::Char('c') | KeyCode::Char('C') => {
                let Some(prompt) = self.selected_prompt() else {
                    return Action::None;
                };
                Action::Copy {
                    id: prompt.metadata.id,
                    title: prompt.metadata.title.clone(),
                    text: self.rendered_prompt().unwrap_or_default(),
                    exit: matches!(key.code, KeyCode::Char('C')),
                }
            }
            KeyCode::Char('p') => {
                let Some(prompt) = self.selected_prompt() else {
                    return Action::None;
                };
                Action::Print {
                    id: prompt.metadata.id,
                    title: prompt.metadata.title.clone(),
                    text: self.rendered_prompt().unwrap_or_default(),
                }
            }
            KeyCode::Up => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                self.preview_scroll = self.preview_scroll.saturating_add(1);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_new_prompt(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Home;
                Action::None
            }
            KeyCode::Backspace => {
                match self.new_field {
                    NewField::Title => self.new_title.pop(),
                    NewField::Tags => self.new_tags.pop(),
                };
                Action::None
            }
            KeyCode::BackTab => {
                self.new_field = NewField::Title;
                Action::None
            }
            KeyCode::Tab | KeyCode::Enter if self.new_field == NewField::Title => {
                if self.new_title.trim().is_empty() {
                    self.notice = Some("A title is required".into());
                } else {
                    self.new_field = NewField::Tags;
                }
                Action::None
            }
            KeyCode::Enter => Action::NewPrompt {
                title: self.new_title.trim().to_owned(),
                tags: parse_tags(&self.new_tags),
            },
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                match self.new_field {
                    NewField::Title => self.new_title.push(character),
                    NewField::Tags => self.new_tags.push(character),
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn open_prompt(&mut self, index: usize, from_search: bool) {
        let prompt = &self.prompts[index];
        self.selected_id = Some(prompt.metadata.id);
        self.variable_names = template::variables(&prompt.body);
        self.variable_values.clear();
        self.variable_index = 0;
        self.variable_input.clear();
        self.preview_scroll = 0;
        self.return_to_search = from_search;
        self.screen = if self.variable_names.is_empty() {
            Screen::Preview
        } else {
            Screen::Variables
        };
    }

    fn refresh_search(&mut self) {
        self.search_hits = search(&self.prompts, &self.usage, &self.search_query);
        self.search_selected = self
            .search_selected
            .min(self.search_hits.len().saturating_sub(1));
    }

    fn store_current_variable(&mut self) {
        if let Some(name) = self.variable_names.get(self.variable_index) {
            self.variable_values
                .insert(name.clone(), self.variable_input.clone());
        }
    }

    fn load_current_variable(&mut self) {
        self.variable_input = self
            .variable_names
            .get(self.variable_index)
            .and_then(|name| self.variable_values.get(name))
            .cloned()
            .unwrap_or_default();
    }

    fn commit_variable(&mut self) {
        self.store_current_variable();
        if self.variable_index + 1 >= self.variable_names.len() {
            self.screen = Screen::Preview;
        } else {
            self.variable_index += 1;
            self.load_current_variable();
        }
    }

    fn return_to_list(&mut self) {
        self.screen = if self.return_to_search {
            Screen::Search
        } else {
            Screen::Home
        };
    }

    fn open_help(&mut self) {
        self.help_return = self.screen;
        self.help_scroll = 0;
        self.screen = Screen::Help;
    }

    fn handle_help(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.screen = self.help_return;
                Action::None
            }
            KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                self.help_scroll = self.help_scroll.saturating_add(1);
                Action::None
            }
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::KeyEvent;

    use crate::model::PromptMetadata;

    use super::*;

    fn prompt(title: &str, body: &str) -> Prompt {
        Prompt {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title: title.into(),
                tags: vec![],
                aliases: vec![],
            },
            body: body.into(),
            path: PathBuf::new(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn paths() -> WalletPaths {
        WalletPaths::from_root("/test-root")
    }

    #[test]
    fn numeric_shortcut_opens_the_matching_frequent_prompt() {
        let first = prompt("First", "one");
        let second = prompt("Second", "two");
        let mut usage = UsageStore::default();
        usage.record_at(second.metadata.id, 1);
        let mut app = App::new(
            Vault {
                prompts: vec![first, second.clone()],
                warnings: vec![],
            },
            usage,
            paths(),
        );

        app.handle_key(key(KeyCode::Char('1')));

        assert_eq!(app.selected_id, Some(second.metadata.id));
        assert_eq!(app.screen, Screen::Preview);
    }

    #[test]
    fn q_is_search_text_not_a_quit_command_while_searching() {
        let mut app = App::new(Vault::default(), UsageStore::default(), paths());
        app.handle_key(key(KeyCode::Char('s')));

        let action = app.handle_key(key(KeyCode::Char('q')));

        assert_eq!(action, Action::None);
        assert_eq!(app.search_query, "q");
        assert_eq!(app.screen, Screen::Search);
    }

    #[test]
    fn variable_flow_asks_once_and_produces_copy_action_with_rendered_text() {
        let prompt = prompt("Greeting", "Hello {{name}}. Again, {{ name }}!");
        let id = prompt.metadata.id;
        let mut app = App::new(
            Vault {
                prompts: vec![prompt],
                warnings: vec![],
            },
            UsageStore::default(),
            paths(),
        );
        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.variable_names, ["name"]);
        for character in "Ada".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        let action = app.handle_key(key(KeyCode::Char('c')));

        assert_eq!(app.screen, Screen::Preview);
        assert_eq!(
            action,
            Action::Copy {
                id,
                title: "Greeting".into(),
                text: "Hello Ada. Again, Ada!".into(),
                exit: false,
            }
        );
        assert_eq!(
            app.usage.entry(&id).use_count,
            0,
            "selection alone is not usage"
        );
    }

    #[test]
    fn successful_copy_is_the_only_point_that_records_usage() {
        let prompt = prompt("Greeting", "Hello");
        let id = prompt.metadata.id;
        let mut app = App::new(
            Vault {
                prompts: vec![prompt],
                warnings: vec![],
            },
            UsageStore::default(),
            paths(),
        );

        app.record_successful_use(id, "Greeting");

        assert_eq!(app.usage.entry(&id).use_count, 1);
        assert_eq!(app.exit_status, "✓ Copied “Greeting” to clipboard");
    }

    #[test]
    fn question_mark_opens_help_and_esc_returns_to_the_previous_screen() {
        let mut app = App::new(Vault::default(), UsageStore::default(), paths());

        let action = app.handle_key(key(KeyCode::Char('?')));

        assert_eq!(action, Action::None);
        assert_eq!(app.screen, Screen::Help);

        app.handle_key(key(KeyCode::Esc));

        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn help_text_explains_the_screens_and_lists_the_resolved_file_paths() {
        let app = App::new(Vault::default(), UsageStore::default(), paths());

        let text = app.help_text();

        assert!(text.contains("Search"));
        assert!(text.contains("Variables"));
        assert!(text.contains("Preview"));
        assert!(text.contains("/test-root/config.toml"));
        assert!(text.contains("/test-root/prompts"));
        assert!(text.contains("/test-root/drafts"));
        assert!(text.contains("/test-root/usage.json"));
    }
}
