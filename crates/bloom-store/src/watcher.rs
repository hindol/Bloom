use bloom_error::BloomError;
use crate::traits::FileEvent;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;

/// Start a file system watcher on `root`, forwarding events as `FileEvent` on `tx`.
pub fn start_watcher(
    root: &Path,
    tx: crossbeam::channel::Sender<FileEvent>,
) -> Result<notify::RecommendedWatcher, BloomError> {
    let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        let Ok(event) = res else { return };
        let paths = event.paths;
        match event.kind {
            EventKind::Create(_) => {
                for p in paths {
                    let _ = tx.send(FileEvent::Created(p));
                }
            }
            EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::Both))
                if paths.len() >= 2 =>
            {
                let _ = tx.send(FileEvent::Renamed {
                    from: paths[0].clone(),
                    to: paths[1].clone(),
                });
            }
            EventKind::Modify(_) => {
                for p in paths {
                    let _ = tx.send(FileEvent::Modified(p));
                }
            }
            EventKind::Remove(_) => {
                for p in paths {
                    let _ = tx.send(FileEvent::Deleted(p));
                }
            }
            _ => {}
        }
    })?;

    // We need a mutable binding to call watch()
    let mut watcher = watcher;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(watcher)
}
