//! Phase-decomposition benchmark for the raw-MFT scan path: prints how long
//! each pipeline layer takes in isolation (device read, chunk layer, fixups,
//! parse, full streaming emit) so optimization work targets the real cost.

use std::path::PathBuf;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\"));
    let workers = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);

    match winblaze_scanner::ntfs::profile_mft_phases(&root, workers) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("profile failed: {error:?}");
            std::process::exit(1);
        }
    }
}
