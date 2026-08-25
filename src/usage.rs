use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::atomic_write;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEntry {
    pub use_count: u64,
    pub last_used_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageStore {
    #[serde(default)]
    entries: BTreeMap<Uuid, UsageEntry>,
}

impl UsageStore {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents)
                .with_context(|| format!("invalid usage file: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
        }
    }

    pub fn entry(&self, id: &Uuid) -> UsageEntry {
        self.entries.get(id).cloned().unwrap_or_default()
    }

    pub fn record_at(&mut self, id: Uuid, timestamp: u64) {
        let entry = self.entries.entry(id).or_default();
        entry.use_count = entry.use_count.saturating_add(1);
        entry.last_used_at = timestamp;
    }

    pub fn record_now(&mut self, id: Uuid) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_secs();
        self.record_at(id, timestamp);
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_json::to_vec_pretty(self).context("could not serialize usage")?;
        atomic_write(path, &contents)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn counts_usage_and_keeps_latest_timestamp() {
        let id = Uuid::new_v4();
        let mut store = UsageStore::default();

        store.record_at(id, 10);
        store.record_at(id, 25);

        assert_eq!(
            store.entry(&id),
            UsageEntry {
                use_count: 2,
                last_used_at: 25
            }
        );
    }

    #[test]
    fn persists_and_loads_usage_without_losing_ids() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usage.json");
        let id = Uuid::new_v4();
        let mut store = UsageStore::default();
        store.record_at(id, 42);
        store.save(&path).unwrap();

        let loaded = UsageStore::load(&path).unwrap();

        assert_eq!(loaded.entry(&id), store.entry(&id));
    }

    #[test]
    fn missing_usage_file_starts_empty_but_malformed_file_is_reported() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usage.json");
        assert_eq!(UsageStore::load(&path).unwrap(), UsageStore::default());

        fs::write(&path, "not json").unwrap();
        assert!(UsageStore::load(&path).is_err());
    }
}
