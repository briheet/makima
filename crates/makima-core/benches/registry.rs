#![expect(clippy::expect_used, reason = "setup failures should stop the bench")]

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use makima_core::{AgentRole, AgentStatus, Registry};

fn status_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("set_agent_status");
    for agents in [10, 50, 200] {
        let mut registry = Registry::new();
        let ws = registry.create_workspace("bench").expect("valid name").id();
        let ids: Vec<_> = (0..agents)
            .map(|i| {
                registry
                    .add_agent(ws, &format!("agent-{i}"), AgentRole::Shell, None)
                    .expect("workspace exists")
                    .id()
            })
            .collect();

        group.throughput(Throughput::Elements(agents));
        group.bench_with_input(BenchmarkId::from_parameter(agents), &ids, |b, ids| {
            b.iter(|| {
                for id in ids {
                    let _ = black_box(registry.set_agent_status(*id, AgentStatus::Working));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, status_updates);
criterion_main!(benches);
