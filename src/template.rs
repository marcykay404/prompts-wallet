use std::{collections::BTreeMap, sync::LazyLock};

use regex::{Captures, Regex};

static VARIABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_-]*)\s*\}\}").expect("variable regex must compile")
});

pub fn variables(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for captures in VARIABLE.captures_iter(body) {
        let name = captures[1].to_owned();
        if !found.contains(&name) {
            found.push(name);
        }
    }
    found
}

pub fn render(body: &str, values: &BTreeMap<String, String>) -> String {
    VARIABLE
        .replace_all(body, |captures: &Captures<'_>| {
            values
                .get(&captures[1])
                .cloned()
                .unwrap_or_else(|| captures[0].to_owned())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_unique_variables_in_first_seen_order() {
        let result = variables("{{language}} {{ audience }} {{language}} {{topic-name}}");
        assert_eq!(result, ["language", "audience", "topic-name"]);
    }

    #[test]
    fn renders_repeated_and_multiline_values_without_recursive_expansion() {
        let mut values = BTreeMap::new();
        values.insert(
            "code".into(),
            "fn main() {\n  println!(\"{{literal}}\");\n}".into(),
        );

        let result = render("Review:\n{{ code }}\nAgain: {{code}}", &values);

        assert_eq!(
            result,
            "Review:\nfn main() {\n  println!(\"{{literal}}\");\n}\nAgain: fn main() {\n  println!(\"{{literal}}\");\n}"
        );
    }

    #[test]
    fn leaves_missing_values_unchanged() {
        assert_eq!(render("Hello {{name}}", &BTreeMap::new()), "Hello {{name}}");
    }
}
