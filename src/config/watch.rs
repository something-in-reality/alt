//! Reloads the configuration when its file changes.

use std::{
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

/// Loads `path` into `config` on each change to it; an invalid file keeps the last valid
/// configuration. The watch is on the file's directory, which sees the file replaced by a link.
pub fn watch(
    path: PathBuf,
    config: Arc<ArcSwap<Config>>,
) -> notify::Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let path = std::path::absolute(path)?;
    let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
    let name = path.file_name().map(ToOwned::to_owned);
    let mut debouncer = new_debouncer(
        Duration::from_millis(250),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let changed = events
                    .iter()
                    .flat_map(|event| &event.paths)
                    .any(|changed| changed.file_name() == name.as_deref());
                if changed {
                    match Config::load(&path) {
                        Ok(loaded) => {
                            config.store(Arc::new(loaded));
                            tracing::info!(path = %path.display(), "reloaded");
                        }
                        Err(error) => tracing::error!(
                            error = %format_args!("{:#}", anyhow::Error::from(error)),
                            "keeping the last configuration"
                        ),
                    }
                }
            }
            Err(errors) => errors
                .iter()
                .for_each(|error| tracing::error!(%error, "watching the configuration")),
        },
    )?;
    debouncer.watch(&directory, RecursiveMode::NonRecursive)?;
    Ok(debouncer)
}
