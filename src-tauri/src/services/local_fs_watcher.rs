use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::commands::local_fs::ensure_local_storage_root;

const LOCAL_FS_CHANGED_EVENT: &str = "local-fs-changed";
const LOCAL_FS_DEBOUNCE_MS: u64 = 150;

#[derive(Clone, Serialize)]
struct LocalFsChangedEvent {
    path: &'static str,
}

pub fn spawn_local_storage_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let root = match ensure_local_storage_root() {
            Ok(root) => root,
            Err(err) => {
                eprintln!("[local-fs-watcher] storage root unavailable: {err}");
                return;
            }
        };

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = match RecommendedWatcher::new(
            move |result| {
                let _ = tx.send(result);
            },
            Config::default(),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                eprintln!("[local-fs-watcher] watcher unavailable: {err}");
                return;
            }
        };

        if let Err(err) = watcher.watch(&root, RecursiveMode::Recursive) {
            eprintln!(
                "[local-fs-watcher] watch failed for {}: {err}",
                root.display()
            );
            return;
        }

        let debounce = Duration::from_millis(LOCAL_FS_DEBOUNCE_MS);
        let mut changed = false;
        loop {
            match rx.recv_timeout(debounce) {
                Ok(Ok(_)) => changed = true,
                Ok(Err(err)) => eprintln!("[local-fs-watcher] watch event failed: {err}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if changed {
                        changed = false;
                        let _ = app.emit(LOCAL_FS_CHANGED_EVENT, LocalFsChangedEvent { path: "/" });
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}
