use std::{
    path::PathBuf,
    sync::{mpsc, Arc},
    thread,
    time::SystemTime,
};

use notify::event::{CreateKind, MetadataKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;

/// Represents file-level changes we care about for triggering sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Removed,
    Renamed { from: PathBuf, to: PathBuf },
    Metadata,
    Other,
}

/// Normalized file event emitted to sinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvent {
    pub path: PathBuf,
    pub kind: FileChangeKind,
    pub occurred_at: SystemTime,
}

/// Sinks receive normalized file events; typically the sync orchestrator implements this.
pub trait FileEventSink: Send + Sync + 'static {
    fn handle(&self, event: FileEvent);
}

#[derive(Debug, Error)]
pub enum FileMonitorError {
    #[error("no paths provided to monitor")]
    NoPaths,
    #[error(transparent)]
    Notify(#[from] notify::Error),
}

/// In-memory watcher manager that keeps recommended platform-specific watchers alive.
///
/// It does not assume folder ownership; you can watch arbitrary file paths or directories.
/// Events are delivered immediately to the provided sink without user interaction.
pub struct FileMonitor {
    _watchers: Vec<RecommendedWatcher>,
    _worker: thread::JoinHandle<()>,
}

impl FileMonitor {
    /// Start monitoring the provided paths (files or directories) and forward normalized events
    /// to the given sink. Uses platform-specific backends provided by `notify`.
    pub fn watch<S: FileEventSink>(
        paths: impl IntoIterator<Item = PathBuf>,
        sink: Arc<S>,
    ) -> Result<Self, FileMonitorError> {
        let mut watchers = Vec::new();
        let (tx, rx) = mpsc::channel();

        let mut any = false;
        for path in paths {
            any = true;
            let tx = tx.clone();
            let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
            // Non-recursive by default to avoid unintended folder ownership; caller can pass a directory
            // and set recursion explicitly via `watch_recursive`.
            watcher.watch(&path, RecursiveMode::NonRecursive)?;
            watchers.push(watcher);
        }
        if !any {
            return Err(FileMonitorError::NoPaths);
        }

        let worker_sink = sink.clone();
        let worker = thread::spawn(move || {
            for res in rx {
                match res {
                    Ok(event) => {
                        if let Some(normalized) = normalize_event(event) {
                            worker_sink.handle(normalized);
                        }
                    }
                    Err(_recv_err) => break,
                }
            }
        });

        Ok(Self {
            _watchers: watchers,
            _worker: worker,
        })
    }

    /// Watch a directory recursively (opt-in). This can be used for higher-level workflows that
    /// still avoid claiming ownership—callers choose the directory explicitly.
    pub fn watch_recursive<S: FileEventSink>(
        path: PathBuf,
        sink: Arc<S>,
    ) -> Result<Self, FileMonitorError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
        watcher.watch(&path, RecursiveMode::Recursive)?;

        let worker_sink = sink.clone();
        let worker = thread::spawn(move || {
            for res in rx {
                match res {
                    Ok(event) => {
                        if let Some(normalized) = normalize_event(event) {
                            worker_sink.handle(normalized);
                        }
                    }
                    Err(_recv_err) => break,
                }
            }
        });

        Ok(Self {
            _watchers: vec![watcher],
            _worker: worker,
        })
    }
}

fn normalize_event(event: Event) -> Option<FileEvent> {
    // Many backends emit multiple paths; we derive a primary path and classify.
    let occurred_at = SystemTime::now();
    let kind = match &event.kind {
        EventKind::Create(CreateKind::File | CreateKind::Any | CreateKind::Other) => {
            FileChangeKind::Created
        }
        EventKind::Modify(
            ModifyKind::Data(_)
            | ModifyKind::Any
            | ModifyKind::Other
            | ModifyKind::Name(RenameMode::Both),
        ) => FileChangeKind::Modified,
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)) => FileChangeKind::Metadata,
        EventKind::Remove(RemoveKind::File | RemoveKind::Any | RemoveKind::Other) => {
            FileChangeKind::Removed
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // Expect two paths: from, to. If missing, degrade to Other.
            if event.paths.len() == 2 {
                FileChangeKind::Renamed {
                    from: event.paths[0].clone(),
                    to: event.paths[1].clone(),
                }
            } else {
                FileChangeKind::Other
            }
        }
        _ => FileChangeKind::Other,
    };

    let path = event.paths.get(0).cloned().unwrap_or_else(PathBuf::new);
    Some(FileEvent {
        path,
        kind,
        occurred_at,
    })
}

/// Example sink useful for tests or hooking into the sync layer.
pub struct ChannelSink {
    pub sender: mpsc::Sender<FileEvent>,
}

impl FileEventSink for ChannelSink {
    fn handle(&self, event: FileEvent) {
        let _ = self.sender.send(event);
    }
}
