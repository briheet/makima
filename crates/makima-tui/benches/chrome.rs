//! The chrome's hot paths: rendering a frame (every repaint), the new-work
//! form, key handling and fuzzy filtering.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use makima_core::AgentId;
use makima_core::{builtin_presets, builtin_templates};
use makima_tui::{
    ChromeMode, ChromeState, FloatSpec, FlowOptions, KeyOutcome, KeyStyle, Keymap, NewWorkFlow,
    PaneRole, PaneSpec, PaneStatus, Picker, PickerItem, StatusCounts, TabSpec, TalkLine, ToolKey,
    render,
};

/// A work with a lead and `children` children, as the app would show it.
fn team(children: u16) -> ChromeState {
    let panes = (0..=children)
        .map(|i| PaneSpec {
            id: AgentId::new(),
            name: format!("claude {}", i + 1),
            role: if i == 0 {
                PaneRole::Lead
            } else {
                PaneRole::Child(i)
            },
            status: PaneStatus::Working,
            detail: "Claude Code".into(),
            focused: i == 0,
            unguarded: true,
        })
        .collect();
    ChromeState {
        tabs: vec![TabSpec {
            label: "makima".into(),
            active: true,
            attention: false,
        }],
        panes,
        zoomed: None,
        mode: ChromeMode::Pane,
        talk: TalkLine {
            prompt: String::new(),
            text: String::new(),
            cursor: 0,
        },
        counts: StatusCounts {
            working: children + 1,
            ..StatusCounts::default()
        },
        work_name: Some("makima".into()),
        clock: "21:00".into(),
        show_help: false,
        confirm: None,
        notice: None,
        key_style: KeyStyle::Helix,
        menu: None,
        bindings: Vec::new(),
        float: None,
        leader: "^Space".into(),
    }
}

fn render_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");
    // A laptop-size window and a large external display, at 14pt.
    for (label, columns, rows) in [("146x48", 146, 48), ("240x70", 240, 70)] {
        // Per cell drawn, so the two sizes compare.
        group.throughput(Throughput::Elements(u64::from(columns) * u64::from(rows)));
        for children in [2, 8] {
            let state = team(children);
            group.bench_with_input(
                BenchmarkId::new(format!("{label}/agents"), children + 1),
                &state,
                |b, state| b.iter(|| black_box(render(state, None, None, columns, rows))),
            );
        }
        let mut with_menu = team(8);
        let mut keys = Keymap::new(KeyStyle::Helix);
        if let KeyOutcome::Pending(menu) = keys.feed("space") {
            with_menu.mode = ChromeMode::Keys;
            with_menu.menu = Some(menu);
        }
        group.bench_with_input(
            BenchmarkId::new(format!("{label}/menu-overlay"), 9),
            &with_menu,
            |b, state| b.iter(|| black_box(render(state, None, None, columns, rows))),
        );
        let mut with_tool = team(8);
        with_tool.mode = ChromeMode::Tool;
        with_tool.float = Some(FloatSpec::new("lazygit", "~/code/makima", 90));
        group.bench_with_input(
            BenchmarkId::new(format!("{label}/tool-float"), 9),
            &with_tool,
            |b, state| b.iter(|| black_box(render(state, None, None, columns, rows))),
        );
    }
    group.finish();
}

fn new_work_form(c: &mut Criterion) {
    let options = || {
        FlowOptions::new(builtin_presets(), builtin_templates())
            .with_home("/Users/me")
            .with_folders(
                (0..50)
                    .map(|i| format!("/Users/me/code/repo-{i}"))
                    .collect(),
            )
            .modal(true)
    };
    let state = ChromeState {
        mode: ChromeMode::NewWork,
        panes: Vec::new(),
        ..team(0)
    };
    let mut flow = NewWorkFlow::new(options());
    flow.feed(b"t");
    flow.feed(b"\r");
    c.bench_function("render/new-work template form 146x48", |b| {
        b.iter(|| black_box(render(&state, Some(&flow), None, 146, 48)));
    });
    c.bench_function("flow/type one key into the brief", |b| {
        b.iter_batched(
            || {
                let mut flow = NewWorkFlow::new(options());
                for key in [
                    b"t" as &[u8],
                    b"\r",
                    b"j",
                    b"j",
                    b"j",
                    b"j",
                    b"j",
                    b"j",
                    b"i",
                ] {
                    flow.feed(key);
                }
                flow
            },
            |mut flow| black_box(flow.feed(b"x")),
            criterion::BatchSize::SmallInput,
        );
    });
}

fn keys(c: &mut Criterion) {
    let tools = vec![
        ToolKey::new(Some('g'), "lazygit"),
        ToolKey::new(Some('s'), "shell"),
        ToolKey::new(Some('e'), "editor"),
        ToolKey::new(Some('f'), "yazi"),
    ];
    let mut keymap = Keymap::new(KeyStyle::Helix).with_tools(tools);
    c.bench_function("keys/space then g (open a tool)", |b| {
        b.iter(|| {
            keymap.feed(black_box("space"));
            black_box(keymap.feed(black_box("g")))
        });
    });
}

fn fuzzy(c: &mut Criterion) {
    let mut group = c.benchmark_group("picker/filter");
    for count in [100, 2_000] {
        let items: Vec<PickerItem> = (0..count)
            .map(|i| {
                PickerItem::new(
                    format!("{i}"),
                    format!("~/code/project-{i}/service-{}", i % 37),
                    "",
                )
            })
            .collect();
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &items, |b, items| {
            b.iter_batched(
                || Picker::new("work", items.clone()),
                |mut picker| {
                    picker.feed(b"serv");
                    black_box(picker.feed(b"12"))
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, render_frames, new_work_form, keys, fuzzy);
criterion_main!(benches);
