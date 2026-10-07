use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use makima_core::color::Rgb;
use makima_core::config::{Config, DEFAULT_CONFIG};
use makima_core::tokens::{DesignTokens, ThemePalette, TokenOptions};

fn derive(c: &mut Criterion) {
    let palette = ThemePalette::new(
        Rgb::new(16, 16, 16),
        Rgb::new(255, 255, 255),
        None,
        [Rgb::new(120, 160, 200); 16],
    );
    c.bench_function("derive_design_tokens", |b| {
        b.iter(|| DesignTokens::derive(black_box(&palette), TokenOptions::default()));
    });
}

fn parse_config(c: &mut Criterion) {
    c.bench_function("parse_default_config", |b| {
        b.iter(|| Config::from_toml(black_box(DEFAULT_CONFIG)));
    });
}

criterion_group!(benches, derive, parse_config);
criterion_main!(benches);
