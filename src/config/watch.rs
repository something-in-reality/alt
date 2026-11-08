//! Reloads the configuration when its file changes.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;
use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{self, RecommendedWatcher, RecursiveMode},
};

use super::Config;

/// Loads `path` into `config` once the watch starts and on each change to it; a missing or invalid
/// file keeps the current configuration. The watch covers the directories of `path` and of the file
/// it links to, which see the link replaced and the linked file edited.
pub fn watch(
    path: PathBuf,
    config: Arc<ArcSwap<Config>>,
) -> notify::Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let path = std::path::absolute(path)?;
    let paths: BTreeSet<PathBuf> = [Some(path.clone()), fs::canonicalize(&path).ok()]
        .into_iter()
        .flatten()
        .collect();
    let directories: BTreeSet<PathBuf> = paths
        .iter()
        .map(|path| path.parent().unwrap_or(Path::new("/")).to_owned())
        .collect();
    let names: BTreeSet<_> = paths
        .iter()
        .filter_map(|path| path.file_name())
        .map(ToOwned::to_owned)
        .collect();
    let (watched, live) = (path.clone(), config.clone());
    let mut debouncer = new_debouncer(
        Duration::from_millis(250),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let changed = events
                    .iter()
                    .flat_map(|event| &event.paths)
                    .any(|changed| changed.file_name().is_some_and(|name| names.contains(name)));
                if changed {
                    reload(&watched, &live);
                }
            }
            Err(errors) => errors
                .iter()
                .for_each(|error| tracing::error!(%error, "watching the configuration")),
        },
    )?;
    for directory in &directories {
        fs::create_dir_all(directory)?;
        debouncer.watch(directory, RecursiveMode::NonRecursive)?;
    }
    reload(&path, &config);
    Ok(debouncer)
}

/// Loads `path` into `config`, keeping the current configuration when `path` does not load.
fn reload(path: &Path, config: &ArcSwap<Config>) {
    match Config::load(path) {
        Ok(loaded) => {
            config.store(Arc::new(loaded));
            tracing::info!(path = %path.display(), "loaded");
        }
        Err(error) => tracing::error!(%error, "keeping the current configuration"),
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Instant};

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Polls until `done` holds for the live configuration.
    fn eventually(config: &ArcSwap<Config>, done: impl Fn(&Config) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < TIMEOUT {
            if done(&config.load()) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn an_invalid_file_keeps_the_last_valid_configuration() {
        let dir = std::env::temp_dir().join(format!("alt-watch-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let natural = |config: &Config| config.mouse.natural_scrolling;
        fs::write(&path, "[mouse]\nnatural_scrolling = false").unwrap();
        let config = Arc::<ArcSwap<Config>>::default();
        let _watcher = watch(path.clone(), config.clone()).unwrap();
        assert_eq!(natural(&config.load()), Some(false));

        fs::write(&path, "[mouse]\nnatural_scrolling = true").unwrap();
        assert!(eventually(&config, |c| natural(c) == Some(true)));

        fs::write(&path, "[bindsym]\nf1 = []").unwrap();
        thread::sleep(Duration::from_secs(1));
        assert_eq!(natural(&config.load()), Some(true));

        fs::write(&path, "[mouse]\nnatural_scrolling = false").unwrap();
        assert!(eventually(&config, |c| natural(c) == Some(false)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_starts_the_default_configuration_until_it_exists() {
        let dir = std::env::temp_dir().join(format!("alt-watch-missing-{}", std::process::id()));
        let path = dir.join("alt").join("config.toml");
        let config = Arc::<ArcSwap<Config>>::default();
        let _watcher = watch(path.clone(), config.clone()).unwrap();
        assert_eq!(config.load().mouse.natural_scrolling, None);

        fs::write(&path, "[mouse]\nnatural_scrolling = true").unwrap();
        assert!(eventually(&config, |c| c.mouse.natural_scrolling == Some(true)));
        fs::remove_dir_all(&dir).unwrap();
    }
}
