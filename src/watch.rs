//! Reloads the configuration when its file changes.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::config::Config;

/// Watches `path`'s directory, as home-manager replaces the file by relinking it.
/// Each change loads the file into `config`; an invalid file keeps the last valid configuration.
pub fn watch(
    path: PathBuf,
    config: Arc<ArcSwap<Config>>,
) -> notify::Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let directory = path.parent().unwrap_or(Path::new(".")).to_owned();
    let mut debouncer = new_debouncer(
        Duration::from_millis(250),
        None,
        move |result: DebounceEventResult| {
            let changed =
                result.is_ok_and(|events| events.iter().any(|event| event.paths.contains(&path)));
            if changed {
                match Config::load(&path) {
                    Ok(loaded) => {
                        config.store(Arc::new(loaded));
                        tracing::info!(path = %path.display(), "reloaded");
                    }
                    Err(error) => {
                        tracing::error!(error = %anyhow::Error::from(error), "keeping the last configuration")
                    }
                }
            }
        },
    )?;
    debouncer.watch(&directory, RecursiveMode::NonRecursive)?;
    Ok(debouncer)
}
