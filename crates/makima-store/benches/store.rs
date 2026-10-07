//! Storage paths that run on user actions: saving a message, opening the
//! new-work screen (frecent folders) and finding repos.

#![expect(clippy::expect_used, reason = "setup failures should stop the bench")]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use makima_core::{AgentId, AgentRole, WorkspaceId};
use makima_store::{AgentRow, NewWork, Paths, Store, discover_repos};

const WORKS: u64 = 200;

fn store(c: &mut Criterion) {
    let root = tempfile::tempdir().expect("temp dir");
    let store = Store::open(Paths::under(root.path())).expect("store");
    for i in 0..WORKS {
        store
            .create_work(
                &NewWork::new(WorkspaceId::new(), format!("work {i}"))
                    .with_folder(Some(format!("/Users/me/code/repo-{}", i % 40))),
            )
            .expect("work");
    }
    let work = store
        .create_work(
            &NewWork::new(WorkspaceId::new(), "bench").with_folder(Some("/tmp/app".into())),
        )
        .expect("work");
    let agent = AgentId::new();
    work.add_agent(&AgentRow::new(agent, 0, "claude", AgentRole::Planner))
        .expect("agent");

    let mut group = c.benchmark_group("store");
    group.bench_function("save a message to work.db", |b| {
        b.iter(|| work.add_message(black_box("run the tests"), &[agent], None));
    });
    group.throughput(Throughput::Elements(WORKS));
    group.bench_function("frecent folders over the index", |b| {
        b.iter(|| black_box(store.index().frecent_folders(20)));
    });
    group.throughput(Throughput::Elements(1));
    group.bench_function("create a work (folder + db + index)", |b| {
        b.iter(|| {
            black_box(store.create_work(
                &NewWork::new(WorkspaceId::new(), "new").with_folder(Some("/tmp/x".into())),
            ))
        });
    });
    group.finish();
}

/// A synthetic home: `ORGS` folders of `REPOS` repos, each with a few
/// source folders, so the numbers don't depend on the machine's `~`.
fn fake_home() -> (tempfile::TempDir, u64) {
    const ORGS: usize = 20;
    const REPOS: usize = 25;
    let home = tempfile::tempdir().expect("temp dir");
    let mut folders = 0;
    for org in 0..ORGS {
        for repo in 0..REPOS {
            let repo = home.path().join(format!("code/org-{org}/repo-{repo}"));
            for sub in [".git", "src/app", "docs", "tests"] {
                std::fs::create_dir_all(repo.join(sub)).expect("folder");
                folders += 1;
            }
        }
    }
    (home, folders)
}

fn discovery(c: &mut Criterion) {
    let (home, folders) = fake_home();
    let roots = [home.path().to_owned()];
    let mut group = c.benchmark_group("discover");
    group.sample_size(20);
    group.throughput(Throughput::Elements(folders));
    group.bench_function("500 repos (depth 5)", |b| {
        b.iter(|| {
            let repos = discover_repos(&roots, 5, 2_000);
            assert_eq!(repos.len(), 500);
            black_box(repos)
        });
    });
    group.finish();
}

criterion_group!(benches, store, discovery);
criterion_main!(benches);
