//! Times repo discovery under the home folder:
//! `cargo run --release --example discover_home -p makima-store -- DEPTH`.

fn main() {
    let depth = std::env::args()
        .nth(1)
        .and_then(|d| d.parse().ok())
        .unwrap_or(4);
    let Ok(home) = makima_store::expand_tilde("~") else {
        return;
    };
    let start = std::time::Instant::now();
    let repos = makima_store::discover_repos(&[home], depth, 10_000);
    println!(
        "depth={depth} repos={} secs={:.2}",
        repos.len(),
        start.elapsed().as_secs_f64()
    );
}
