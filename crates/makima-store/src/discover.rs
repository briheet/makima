use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use walkdir::WalkDir;

/// Folders never searched: dependencies, build output, and the parts of a
/// macOS home that hold apps and media rather than code.
const SKIPPED: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    "vendor",
    "Pods",
    "DerivedData",
    "Library",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
];

/// Git repositories under `roots`, at most `depth` levels down, in path
/// order. Repos are searched inside too, so nested repos and submodules
/// are found. Roots that don't exist, and folders that can't be read, are
/// skipped.
#[must_use]
pub fn discover_repos(roots: &[PathBuf], depth: usize, limit: usize) -> Vec<PathBuf> {
    discover_repos_until(roots, depth, limit, &|| false)
}

/// [`discover_repos`], giving up with what it has once `stop` says so.
fn discover_repos_until(
    roots: &[PathBuf],
    depth: usize,
    limit: usize,
    stop: &dyn Fn() -> bool,
) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    for root in roots.iter().filter(|r| r.is_dir()) {
        let mut walker = WalkDir::new(root)
            .max_depth(depth)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter();
        while let Some(entry) = walker.next() {
            if stop() {
                return repos;
            }
            // Unreadable folders (macOS protects ~/Library, ~/Desktop, …)
            // are skipped, not the end of the search.
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path();
            if entry.depth() > 0 && is_skipped(path) {
                walker.skip_current_dir();
                continue;
            }
            // `.git` is a folder in a repo and a file in a submodule.
            if path.join(".git").exists() {
                repos.push(path.to_owned());
                if repos.len() >= limit {
                    return repos;
                }
            }
        }
    }
    repos
}

/// [`discover_repos`] on a background thread, so opening the new-work
/// screen never waits on the disk. The last finished scan is kept, and
/// [`Self::refresh`] starts another to pick up new clones. The owner calls
/// [`Self::stop`] when it's done, which ends and joins a running scan.
#[derive(Debug)]
pub struct RepoScan {
    roots: Vec<PathBuf>,
    depth: usize,
    limit: usize,
    state: Mutex<ScanState>,
    finished: Condvar,
    stopped: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Default)]
struct ScanState {
    repos: Option<Vec<PathBuf>>,
    running: bool,
}

impl RepoScan {
    /// A scan of `roots`; nothing runs until [`Self::refresh`].
    #[must_use]
    pub fn new(roots: Vec<PathBuf>, depth: usize, limit: usize) -> Arc<Self> {
        Arc::new(Self {
            roots,
            depth,
            limit,
            state: Mutex::new(ScanState::default()),
            finished: Condvar::new(),
            stopped: AtomicBool::new(false),
            thread: Mutex::new(None),
        })
    }

    /// Starts a scan unless one is running or the scan was stopped.
    ///
    /// # Errors
    /// The I/O error if the thread can't be started; the last result stays.
    pub fn refresh(self: &Arc<Self>) -> std::io::Result<()> {
        {
            let mut state = self.state();
            if state.running || self.stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            state.running = true;
        }
        let scan = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("makima-repo-scan".into())
            .spawn(move || {
                let stop = || scan.stopped.load(Ordering::Acquire);
                let repos = discover_repos_until(&scan.roots, scan.depth, scan.limit, &stop);
                let mut state = scan.state();
                if !stop() {
                    state.repos = Some(repos);
                }
                state.running = false;
                drop(state);
                scan.finished.notify_all();
            });
        match spawned {
            Ok(thread) => {
                // The previous scan has finished (only one runs at a time).
                let previous = self.thread_slot().replace(thread);
                if let Some(previous) = previous {
                    let _ = previous.join();
                }
                Ok(())
            }
            Err(error) => {
                self.state().running = false;
                Err(error)
            }
        }
    }

    /// Ends a running scan and waits for its thread; later refreshes do
    /// nothing.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let thread = self.thread_slot().take();
        if let Some(thread) = thread {
            // A panicked scan has nothing left to clean up.
            let _ = thread.join();
        }
    }

    fn thread_slot(&self) -> MutexGuard<'_, Option<JoinHandle<()>>> {
        self.thread.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The last finished scan; `None` before the first one finishes.
    #[must_use]
    pub fn repos(&self) -> Option<Vec<PathBuf>> {
        self.state().repos.clone()
    }

    /// Waits up to `timeout` for a running scan, then returns the latest
    /// result.
    #[must_use]
    pub fn wait(&self, timeout: Duration) -> Option<Vec<PathBuf>> {
        let state = self.state();
        let (state, _) = self
            .finished
            .wait_timeout_while(state, timeout, |s| s.running)
            .unwrap_or_else(PoisonError::into_inner);
        state.repos.clone()
    }

    /// The scan only ever stores a finished result, so poisoning is ignored.
    fn state(&self) -> MutexGuard<'_, ScanState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn is_skipped(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|name| name.starts_with('.') || SKIPPED.contains(&name))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn finds_nested_repos_but_not_dependencies() {
        let root = tempfile::tempdir().unwrap();
        for dir in [
            "a/.git",
            "a/sub/.git",
            "group/b/.git",
            "node_modules/x/.git",
            ".hidden/c/.git",
            "plain",
        ] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        let repos = discover_repos(
            &[root.path().to_owned(), PathBuf::from("/does/not/exist")],
            3,
            50,
        );
        let names: Vec<_> = repos
            .iter()
            .map(|p| p.strip_prefix(root.path()).unwrap().display().to_string())
            .collect();
        assert_eq!(names, ["a", "a/sub", "group/b"]);
    }

    #[test]
    fn scans_run_in_the_background() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("app/.git")).unwrap();
        let scan = RepoScan::new(vec![root.path().to_owned()], 3, 50);
        assert_eq!(scan.repos(), None, "nothing until refreshed");
        scan.refresh().unwrap();
        let repos = scan.wait(Duration::from_secs(5)).unwrap();
        assert_eq!(repos, [root.path().join("app")]);

        scan.refresh().unwrap();
        scan.stop();
        assert!(!scan.state().running, "stop joins the scan");
        scan.refresh().unwrap();
        assert!(!scan.state().running, "stopped scans don't restart");
    }
}
