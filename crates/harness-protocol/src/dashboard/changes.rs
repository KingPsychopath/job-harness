//! Disposable change hints. Canonical reads and reviewed writes remain in Engine.
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::watch;

pub(super) struct Changes {
    pub revision: watch::Sender<u64>,
    pub healthy: Arc<AtomicBool>,
    pub closed: watch::Sender<bool>,
    // Keep the OS watcher alive with the dashboard, without a callback ownership cycle.
    _watcher: Mutex<Option<RecommendedWatcher>>,
}

fn relevant(root: &Path, event: &Event) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let Some(first) = relative
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
        else {
            return false;
        };
        if ![
            "applications",
            "context",
            "content",
            "config",
            "sources",
            "output",
            "docs",
            "fonts",
            "resume.typ",
        ]
        .contains(&first)
        {
            return false;
        }
        if relative
            .components()
            .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')))
        {
            return false;
        }
        relative != Path::new("applications").join("pipeline.json")
            && relative != Path::new("applications").join("pipeline.md")
    })
}

impl Changes {
    pub fn new(root: &Path) -> Self {
        let (revision, _) = watch::channel(0_u64);
        let (closed, _) = watch::channel(false);
        let healthy = Arc::new(AtomicBool::new(false));
        let sender = revision.clone();
        let status = healthy.clone();
        let watched = root.to_owned();
        let watcher = RecommendedWatcher::new(
            move |result: notify::Result<Event>| match result {
                Ok(event) if relevant(&watched, &event) || event.need_rescan() => {
                    sender.send_modify(|n| *n = n.wrapping_add(1));
                }
                Err(_) => {
                    status.store(false, Ordering::Release);
                    sender.send_modify(|n| *n = n.wrapping_add(1));
                }
                _ => {}
            },
            Config::default().with_follow_symlinks(false),
        )
        .ok()
        .and_then(|mut watcher| {
            watcher.watch(root, RecursiveMode::Recursive).ok()?;
            healthy.store(true, Ordering::Release);
            Some(watcher)
        });
        Self {
            revision,
            healthy,
            closed,
            _watcher: Mutex::new(watcher),
        }
    }
    pub fn bump(&self) {
        self.revision.send_modify(|n| *n = n.wrapping_add(1));
    }
    pub fn close(&self) {
        self.closed.send_replace(true);
        if let Ok(mut watcher) = self._watcher.lock() {
            watcher.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_ignore_reads_derived_state_and_paths_outside_workspace() {
        let root = Path::new("workspace");
        for ignored in [
            "workspace/.harness/index.sqlite",
            "workspace/.git/HEAD",
            "workspace/applications/operations/.lock",
            "workspace/applications/pipeline.json",
            "other/context/note.md",
        ] {
            let event = Event::new(EventKind::Any).add_path(ignored.into());
            assert!(!relevant(root, &event), "{ignored}");
        }
        for changed in [
            "workspace/applications/operations/state.json",
            "workspace/applications/example.md",
            "workspace/context/intake/note/metadata.json",
            "workspace/config/career-ledger.json",
            "workspace/sources/imported-cv.pdf",
        ] {
            let event = Event::new(EventKind::Any).add_path(changed.into());
            assert!(relevant(root, &event), "{changed}");
        }
        let read = Event::new(EventKind::Access(notify::event::AccessKind::Read))
            .add_path("workspace/context/note.md".into());
        assert!(!relevant(root, &read));
        for name in ["pipeline.json", "pipeline.md"] {
            let event = Event::new(EventKind::Any).add_path(root.join("applications").join(name));
            assert!(
                !relevant(root, &event),
                "Native platform separators must exclude derived pipeline views"
            );
        }
    }
    #[tokio::test]
    async fn filesystem_writes_are_hints_without_reading_or_rewriting_sources() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("context")).unwrap();
        let changes = Changes::new(&root);
        assert!(
            changes.healthy.load(Ordering::Acquire),
            "Local fixture needs native filesystem notifications"
        );
        let mut receiver = changes.revision.subscribe();
        std::fs::write(root.join("context/note.md"), "Exact supplied text").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), receiver.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(root.join("context/note.md")).unwrap(),
            b"Exact supplied text"
        );
        assert!(!root.join("applications").exists());
    }
}
