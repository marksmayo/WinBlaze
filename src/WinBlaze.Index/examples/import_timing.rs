//! Times reopening a saved CSV scan: `cargo run --release -p winblaze-index
//! --example import_timing -- <scan.csv> [runs]`. Prints each run and the median.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: import_timing <scan.csv> [runs]");
    let runs: usize = args.next().map_or(7, |value| value.parse().expect("runs"));
    let mut times = Vec::with_capacity(runs);
    for run in 1..=runs {
        let started = std::time::Instant::now();
        let file = std::io::BufReader::new(std::fs::File::open(&path).expect("open scan"));
        let tree = winblaze_index::import::read_csv(file).expect("import scan");
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!("run {run}: {elapsed:.1} ms, {} files", tree.files().len());
        times.push(elapsed);
    }
    times.sort_by(f64::total_cmp);
    println!("median {:.1} ms", times[times.len() / 2]);
}
