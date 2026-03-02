use crate::store::WriteTracker;
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tif", "tiff"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatcherConfig {
    pub root: PathBuf,
    pub include_images: bool,
}

impl WatcherConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            include_images: true,
        }
    }

    pub fn with_images(mut self, include_images: bool) -> Self {
        self.include_images = include_images;
        self
    }

    pub fn is_path_eligible(&self, path: &Path) -> bool {
        if !path.starts_with(&self.root) {
            return false;
        }

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            return true;
        }

        self.include_images && self.is_image_path(path)
    }

    fn is_image_path(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };

        if !IMAGE_EXTENSIONS
            .iter()
            .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        {
            return false;
        }

        path.strip_prefix(&self.root)
            .ok()
            .and_then(|relative| relative.components().next())
            .is_some_and(|component| component.as_os_str() == "images")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("watch root does not exist: {0}")]
    RootNotFound(PathBuf),
    #[error("watcher backend error: {0}")]
    Notify(#[from] notify::Error),
}

pub struct RunningWatcher {
    _watcher: RecommendedWatcher,
}

/// Start watching `config.root` recursively and emit filtered file events over `events_tx`.
pub fn start(
    config: WatcherConfig,
    events_tx: Sender<WatchEvent>,
    write_tracker: Option<WriteTracker>,
) -> Result<RunningWatcher, WatcherError> {
    if !config.root.exists() {
        return Err(WatcherError::RootNotFound(config.root));
    }

    let watch_root = config.root.clone();
    let write_tracker = write_tracker.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if let Ok(event) = result {
            for mapped in map_notify_event(&config, event) {
                let path = match &mapped {
                    WatchEvent::Created(p)
                    | WatchEvent::Modified(p)
                    | WatchEvent::Removed(p) => p,
                    WatchEvent::Renamed { to, .. } => to,
                };
                if let Some(ref tracker) = write_tracker {
                    if tracker.was_self_write(path) {
                        continue;
                    }
                }
                let _ = events_tx.send(mapped);
            }
        }
    })?;

    watcher.watch(&watch_root, RecursiveMode::Recursive)?;
    Ok(RunningWatcher { _watcher: watcher })
}

fn map_notify_event(config: &WatcherConfig, event: Event) -> Vec<WatchEvent> {
    match event.kind {
        EventKind::Create(_) => map_paths(config, event.paths, WatchEvent::Created),
        EventKind::Modify(ModifyKind::Name(mode)) => map_rename_event(config, mode, event.paths),
        EventKind::Modify(_) => map_paths(config, event.paths, WatchEvent::Modified),
        EventKind::Remove(_) => map_paths(config, event.paths, WatchEvent::Removed),
        _ => Vec::new(),
    }
}

fn map_paths<F>(config: &WatcherConfig, paths: Vec<PathBuf>, map: F) -> Vec<WatchEvent>
where
    F: Fn(PathBuf) -> WatchEvent,
{
    paths
        .into_iter()
        .filter(|path| config.is_path_eligible(path))
        .map(map)
        .collect()
}

fn map_rename_event(config: &WatcherConfig, mode: RenameMode, paths: Vec<PathBuf>) -> Vec<WatchEvent> {
    match mode {
        RenameMode::From => map_paths(config, paths, WatchEvent::Removed),
        RenameMode::To => map_paths(config, paths, WatchEvent::Created),
        RenameMode::Any | RenameMode::Both | RenameMode::Other => map_rename_pair(config, paths),
    }
}

fn map_rename_pair(config: &WatcherConfig, paths: Vec<PathBuf>) -> Vec<WatchEvent> {
    let mut paths = paths.into_iter();
    let Some(from) = paths.next() else {
        return Vec::new();
    };
    let Some(to) = paths.next() else {
        return map_paths(config, vec![from], WatchEvent::Modified);
    };

    match (config.is_path_eligible(&from), config.is_path_eligible(&to)) {
        (true, true) => vec![WatchEvent::Renamed { from, to }],
        (true, false) => vec![WatchEvent::Removed(from)],
        (false, true) => vec![WatchEvent::Created(to)],
        (false, false) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RenameMode};

    fn config() -> WatcherConfig {
        WatcherConfig::new(PathBuf::from("/vault"))
    }

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        paths.iter().fold(Event::new(kind), |event, path| {
            event.add_path(PathBuf::from(path))
        })
    }

    #[test]
    fn path_eligibility_allows_markdown_files_under_root() {
        let cfg = config();
        assert!(cfg.is_path_eligible(Path::new("/vault/pages/note.md")));
        assert!(cfg.is_path_eligible(Path::new("/vault/journal/2026-01-01.MD")));
    }

    #[test]
    fn path_eligibility_rejects_non_markdown_files_outside_images() {
        let cfg = config();
        assert!(!cfg.is_path_eligible(Path::new("/vault/pages/note.txt")));
        assert!(!cfg.is_path_eligible(Path::new("/other/pages/note.md")));
    }

    #[test]
    fn path_eligibility_allows_images_only_in_images_dir_when_enabled() {
        let cfg = config();
        assert!(cfg.is_path_eligible(Path::new("/vault/images/diagram.png")));
        assert!(!cfg.is_path_eligible(Path::new("/vault/pages/diagram.png")));
    }

    #[test]
    fn path_eligibility_rejects_images_when_disabled() {
        let cfg = config().with_images(false);
        assert!(!cfg.is_path_eligible(Path::new("/vault/images/diagram.png")));
    }

    #[test]
    fn filters_create_events_to_eligible_paths() {
        let cfg = config();
        let mapped = map_notify_event(
            &cfg,
            event(
                EventKind::Create(CreateKind::Any),
                &[
                    "/vault/pages/new-note.md",
                    "/vault/pages/ignore.txt",
                    "/outside/pages/other.md",
                ],
            ),
        );

        assert_eq!(
            mapped,
            vec![WatchEvent::Created(PathBuf::from("/vault/pages/new-note.md"))]
        );
    }

    #[test]
    fn maps_markdown_rename_to_renamed_event() {
        let cfg = config();
        let mapped = map_notify_event(
            &cfg,
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["/vault/pages/old.md", "/vault/pages/new.md"],
            ),
        );

        assert_eq!(
            mapped,
            vec![WatchEvent::Renamed {
                from: PathBuf::from("/vault/pages/old.md"),
                to: PathBuf::from("/vault/pages/new.md"),
            }]
        );
    }

    #[test]
    fn maps_rename_from_markdown_to_non_eligible_as_removed() {
        let cfg = config();
        let mapped = map_notify_event(
            &cfg,
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["/vault/pages/old.md", "/vault/pages/new.txt"],
            ),
        );

        assert_eq!(
            mapped,
            vec![WatchEvent::Removed(PathBuf::from("/vault/pages/old.md"))]
        );
    }

    #[test]
    fn maps_rename_into_markdown_as_created() {
        let cfg = config();
        let mapped = map_notify_event(
            &cfg,
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["/vault/pages/old.txt", "/vault/pages/new.md"],
            ),
        );

        assert_eq!(
            mapped,
            vec![WatchEvent::Created(PathBuf::from("/vault/pages/new.md"))]
        );
    }

    #[test]
    fn self_writes_are_filtered_by_tracker() {
        let tracker = crate::store::WriteTracker::new();
        let path = PathBuf::from("/vault/pages/note.md");
        tracker.record(&path);
        assert!(tracker.was_self_write(&path));
        // Second call should return false (entry was consumed)
        assert!(!tracker.was_self_write(&path));
    }
}
