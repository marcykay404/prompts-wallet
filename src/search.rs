use std::cmp::Reverse;

use crate::{model::Prompt, usage::UsageStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit {
    pub index: usize,
    pub relevance: i64,
}

pub fn frequent_indices(prompts: &[Prompt], usage: &UsageStore, limit: usize) -> Vec<usize> {
    let mut indices: Vec<_> = (0..prompts.len()).collect();
    indices.sort_by_key(|index| {
        let entry = usage.entry(&prompts[*index].metadata.id);
        (
            Reverse(entry.use_count),
            Reverse(entry.last_used_at),
            prompts[*index].metadata.title.to_lowercase(),
        )
    });
    indices.truncate(limit);
    indices
}

pub fn search(prompts: &[Prompt], usage: &UsageStore, query: &str) -> Vec<SearchHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return frequent_indices(prompts, usage, prompts.len())
            .into_iter()
            .map(|index| SearchHit {
                index,
                relevance: 0,
            })
            .collect();
    }

    let mut hits: Vec<_> = prompts
        .iter()
        .enumerate()
        .filter_map(|(index, prompt)| {
            prompt_score(prompt, &query).map(|relevance| SearchHit { index, relevance })
        })
        .collect();

    hits.sort_by_key(|hit| {
        let prompt = &prompts[hit.index];
        let entry = usage.entry(&prompt.metadata.id);
        (
            Reverse(hit.relevance),
            Reverse(entry.use_count),
            Reverse(entry.last_used_at),
            prompt.metadata.title.to_lowercase(),
        )
    });
    hits
}

fn prompt_score(prompt: &Prompt, query: &str) -> Option<i64> {
    let title = prompt.metadata.title.to_lowercase();
    if title == query {
        return Some(20_000);
    }
    if prompt
        .metadata
        .aliases
        .iter()
        .any(|alias| alias.to_lowercase() == query)
    {
        return Some(19_000);
    }

    let title_score = fuzzy_score(&title, query).map(|score| 10_000 + score);
    let alias_score = prompt
        .metadata
        .aliases
        .iter()
        .filter_map(|alias| fuzzy_score(&alias.to_lowercase(), query))
        .max()
        .map(|score| 8_000 + score);
    let tag_score = prompt
        .metadata
        .tags
        .iter()
        .filter_map(|tag| fuzzy_score(&tag.to_lowercase(), query))
        .max()
        .map(|score| 6_000 + score);
    let body_score = fuzzy_score(&prompt.body.to_lowercase(), query).map(|score| 2_000 + score);

    [title_score, alias_score, tag_score, body_score]
        .into_iter()
        .flatten()
        .max()
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if let Some(position) = candidate.find(query) {
        return Some(1_000 - position as i64);
    }

    let mut candidate_chars = candidate.char_indices();
    let mut last_position = None;
    let mut gaps = 0_i64;
    for query_char in query.chars() {
        let (position, _) = candidate_chars.find(|(_, character)| *character == query_char)?;
        if let Some(previous) = last_position {
            gaps += position.saturating_sub(previous + 1) as i64;
        }
        last_position = Some(position);
    }
    Some(500 - gaps)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::model::PromptMetadata;

    use super::*;

    fn prompt(title: &str, tags: &[&str], body: &str) -> Prompt {
        Prompt {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title: title.into(),
                tags: tags.iter().map(|tag| (*tag).into()).collect(),
                aliases: vec![],
            },
            body: body.into(),
            path: PathBuf::new(),
        }
    }

    #[test]
    fn home_orders_by_frequency_then_recency() {
        let prompts = vec![prompt("Alpha", &[], "a"), prompt("Beta", &[], "b")];
        let mut usage = UsageStore::default();
        usage.record_at(prompts[0].metadata.id, 10);
        usage.record_at(prompts[1].metadata.id, 20);
        usage.record_at(prompts[1].metadata.id, 21);

        assert_eq!(frequent_indices(&prompts, &usage, 5), [1, 0]);
    }

    #[test]
    fn exact_match_beats_a_more_frequently_used_weak_match() {
        let prompts = vec![
            prompt("Security review", &[], "review"),
            prompt("Frequently used review helper", &["security"], "review"),
        ];
        let mut usage = UsageStore::default();
        for timestamp in 0..100 {
            usage.record_at(prompts[1].metadata.id, timestamp);
        }

        let hits = search(&prompts, &usage, "security review");

        assert_eq!(hits[0].index, 0);
    }

    #[test]
    fn usage_breaks_ties_between_equally_relevant_results() {
        let prompts = vec![
            prompt("Rust helper one", &[], "body"),
            prompt("Rust helper two", &[], "body"),
        ];
        let mut usage = UsageStore::default();
        usage.record_at(prompts[1].metadata.id, 10);

        let hits = search(&prompts, &usage, "rust helper");

        assert_eq!(hits[0].index, 1);
    }

    #[test]
    fn fuzzy_subsequence_matches_non_contiguous_characters() {
        let prompts = vec![prompt("Security review", &[], "body")];
        assert_eq!(search(&prompts, &UsageStore::default(), "scrv").len(), 1);
    }
}
