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

/// Loads `path` into `config` on each change to it, then calls `reloaded`; an invalid file keeps
/// the last valid configuration. The watch covers the directories of `path` and of the file it
/// links to, which see the link replaced and the linked file edited.
pub fn watch(
    path: PathBuf,
    config: Arc<ArcSwap<Config>>,
    reloaded: impl Fn() + Send + 'static,
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
                    match Config::load(&path) {
                        Ok(loaded) => {
                            config.store(Arc::new(loaded));
                            tracing::info!(path = %path.display(), "reloaded");
                            reloaded();
                        }
                        Err(error) => tracing::error!(%error, "keeping the last configuration"),
                    }
                }
            }
            Err(errors) => errors
                .iter()
                .for_each(|error| tracing::error!(%error, "watching the configuration")),
        },
    )?;
    for directory in &directories {
        debouncer.watch(directory, RecursiveMode::NonRecursive)?;
    }
    Ok(debouncer)
}
