use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;
use uuid::Uuid;

fn write_prompt(root: &std::path::Path, title: &str, body: &str) {
    let prompts = root.join("prompts");
    fs::create_dir_all(&prompts).unwrap();
    let id = Uuid::new_v4();
    let contents =
        format!("+++\nid = \"{id}\"\ntitle = \"{title}\"\ntags = []\naliases = []\n+++\n{body}");
    fs::write(prompts.join("prompt.md"), contents).unwrap();
}

#[test]
fn list_reads_prompts_from_portable_home() {
    let root = tempdir().unwrap();
    write_prompt(root.path(), "Security review", "Review this code.\n");

    Command::cargo_bin("pwt")
        .unwrap()
        .env("PW_HOME", root.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Security review"));
}

#[test]
fn show_uses_fuzzy_search_and_prints_only_the_prompt_body() {
    let root = tempdir().unwrap();
    write_prompt(
        root.path(),
        "Security review",
        "Review {{language}} for vulnerabilities.\n",
    );

    Command::cargo_bin("pwt")
        .unwrap()
        .env("PW_HOME", root.path())
        .args(["show", "scrv"])
        .assert()
        .success()
        .stdout("Review {{language}} for vulnerabilities.\n");
}

#[test]
fn paths_are_fully_relocated_by_pw_home() {
    let root = tempdir().unwrap();
    let root_text = root.path().display().to_string();

    Command::cargo_bin("pwt")
        .unwrap()
        .env("PW_HOME", root.path())
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("{root_text}/prompts")))
        .stdout(predicate::str::contains(format!("{root_text}/usage.json")));
}

#[test]
fn show_returns_a_clear_error_when_nothing_matches() {
    let root = tempdir().unwrap();

    Command::cargo_bin("pwt")
        .unwrap()
        .env("PW_HOME", root.path())
        .args(["show", "missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no prompt matched"));
}
