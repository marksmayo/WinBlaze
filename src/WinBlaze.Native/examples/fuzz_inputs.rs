//! Dependency-free mutation fuzzer for every parser that reads untrusted input.
//!
//! Usage: cargo run --release -p winblaze-native --example fuzz_inputs -- [iterations-per-target] [seed]
//! Each target starts from valid seed inputs and applies random mutations
//! (bit flips, byte replacement with boundary values, splices, truncation,
//! duplication, insertion). Panics and inputs slower than one second are
//! reported and saved under target/fuzz-findings/<target>/.
use std::{
    io::Cursor,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord};
use winblaze_index::{
    export::ExportFilter,
    import,
    tree_search::{self, SearchOptions, TextMode},
    BufferedIndexTransaction, IndexBackend, IndexRepository, SqliteIndexRepository, TreeIndex,
};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const INTERESTING: &[u8] = &[
    0, 1, 0x7f, 0x80, 0xff, b'"', b',', b'\t', b'\n', b'\r', b'\\', b'.', b'*', b'?', b'[', b'(',
    b'{', b'0', b'9', b'-', b'+', b':', b'/',
];

fn mutate(rng: &mut Rng, seeds: &[Vec<u8>], max_len: usize) -> Vec<u8> {
    let mut data = seeds[rng.below(seeds.len())].clone();
    for _ in 0..1 + rng.below(8) {
        match rng.below(9) {
            0 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] ^= 1 << rng.below(8);
            }
            1 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] = INTERESTING[rng.below(INTERESTING.len())];
            }
            2 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] = rng.next() as u8;
            }
            3 => data.truncate(rng.below(data.len() + 1)),
            4 if !data.is_empty() => {
                // duplicate a chunk
                let start = rng.below(data.len());
                let end = (start + 1 + rng.below(64)).min(data.len());
                let chunk = data[start..end].to_vec();
                let at = rng.below(data.len() + 1);
                data.splice(at..at, chunk);
            }
            5 => {
                // splice in another seed
                let other = &seeds[rng.below(seeds.len())];
                if !other.is_empty() {
                    let start = rng.below(other.len());
                    let end = (start + 1 + rng.below(other.len() - start)).min(other.len());
                    let at = rng.below(data.len() + 1);
                    data.splice(at..at, other[start..end].iter().copied());
                }
            }
            6 => {
                let at = rng.below(data.len() + 1);
                data.insert(at, INTERESTING[rng.below(INTERESTING.len())]);
            }
            7 if data.len() >= 4 => {
                // overwrite a little-endian u32/u64 field with a boundary value
                let values: [u64; 6] = [
                    0,
                    1,
                    u32::MAX as u64,
                    u64::MAX,
                    1 << 31,
                    0xFFFF_FFFF_FFFF_FFFE,
                ];
                let value = values[rng.below(values.len())].to_le_bytes();
                let width = if data.len() >= 8 && rng.below(2) == 0 {
                    8
                } else {
                    4
                };
                let at = rng.below(data.len() - width + 1);
                data[at..at + width].copy_from_slice(&value[..width]);
            }
            _ => {
                let at = rng.below(data.len() + 1);
                data.insert(at, rng.next() as u8);
            }
        }
    }
    data.truncate(max_len);
    data
}

struct Report {
    target: &'static str,
    runs: u64,
    panics: u64,
    slow: u64,
    max: Duration,
}

fn run_target(
    name: &'static str,
    iterations: u64,
    rng: &mut Rng,
    seeds: Vec<Vec<u8>>,
    max_len: usize,
    mut body: impl FnMut(&[u8]),
) -> Report {
    let findings = PathBuf::from("target/fuzz-findings").join(name);
    let mut report = Report {
        target: name,
        runs: 0,
        panics: 0,
        slow: 0,
        max: Duration::ZERO,
    };
    for seed in &seeds {
        body(seed); // seeds must parse without panicking
    }
    for _ in 0..iterations {
        let input = mutate(rng, &seeds, max_len);
        let started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| body(&input)));
        let elapsed = started.elapsed();
        report.runs += 1;
        report.max = report.max.max(elapsed);
        let slow = elapsed > Duration::from_secs(1);
        if outcome.is_err() || slow {
            if outcome.is_err() {
                report.panics += 1
            } else {
                report.slow += 1
            }
            let _ = std::fs::create_dir_all(&findings);
            let kind = if outcome.is_err() { "panic" } else { "slow" };
            let path = findings.join(format!("{kind}-{}.bin", report.panics + report.slow));
            let _ = std::fs::write(&path, &input);
            eprintln!(
                "[{name}] {kind} after {elapsed:?}: saved {}",
                path.display()
            );
        }
    }
    report
}

fn sample_tree() -> TreeIndex {
    import::read_csv(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n\
         \"Z:\\o\",Directory,30,8192,3,1,1700000000\r\n\
         \"Z:\\o\\sub\",Directory,20,4096,2,0,1700000000\r\n\
         \"Z:\\o\\a.TXT\",File,10,4096,0,0,1700000000\r\n\
         \"Z:\\o\\sub\\B.bin\",File,15,4096,0,0,1600000000\r\n\
         \"Z:\\o\\sub\\\u{e9}t\u{e9}.d\u{e9}\",File,5,0,0,0,\r\n"
            .as_bytes(),
    )
    .expect("seed tree")
}

fn snapshot_seed(dir: &Path) -> Vec<u8> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let repository = SqliteIndexRepository::open(dir, IndexBackend::BinaryCache);
    let mut tx = BufferedIndexTransaction::default();
    tx.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "Z:\\".into(),
        full_path: "Z:\\".into(),
        ..DirectoryRecord::default()
    });
    for i in 0..4 {
        tx.insert_file(FileRecord {
            id: FileId(i + 1),
            parent_directory_id: DirectoryId(1),
            name: format!("file-{i}.dat with a longer name").into(),
            size_bytes: i * 1000,
            allocation_bytes: i * 4096,
            ..FileRecord::default()
        });
    }
    repository
        .persist_transaction(&tx)
        .expect("persist seed snapshot");
    let file = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .expect("snapshot file");
    std::fs::read(file).unwrap()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let iterations: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(20_000);
    let seed: u64 = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x5EED_F00D);
    let mut rng = Rng(seed | 1);
    // Panics are expected findings: keep stderr readable.
    std::panic::set_hook(Box::new(|_| {}));
    let scratch = std::env::temp_dir().join(format!("winblaze-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let mut reports = Vec::new();

    let csv_seeds: Vec<Vec<u8>> = [
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n\"Z:\\offline\",Directory,7,4096,1,0,123\r\n\"Z:\\offline\\x,\"\"y\"\".bin\",File,7,4096,0,0,123\r\n",
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\o\\left\\a.bin,File,7,4096,0,0,\nZ:\\o\\right\\b.bin,File,9,4096,0,0,1767225600\n",
        "Name,Files,Folders,Logical Size,Physical Size,Attributes,Last Change,WinDirStat Attributes,Index,Owner\nZ:\\offline\\a.bin,0,0,7,4096,A,2026-01-01T00:00:00Z,0x20000008,1,DOMAIN\\user\n",
        "Generated by WizTree 4.33\nFile Name,Size,Allocated,Modified,Attributes,Files,Folders\n\"Z:\\offline\\\",7,4096,2026/01/01 13:00:00,16,1,0\n\"Z:\\offline\\a.bin\",7,4096,2026/01/01 13:00:00,32,0,0\n",
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds,Attributes,CreatedUnixSeconds,AccessedUnixSeconds,Owner,VolumeIdentity,FileIdentity,LinkCount\n\"Z:\\o\",Directory,1,1,1,0,,,,,,,,\n\"Z:\\o\\a\",File,1,1,0,0,5,32,4,6,me,7,8,2\n",
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n\"WinBlaze:\\Multiple roots\",Directory,2,2,2,2,\n\"Y:\\a\",Directory,1,1,1,0,\n\"Y:\\a\\f\",File,1,1,0,0,\n\"Z:\\b\",Directory,1,1,1,0,\n\"Z:\\b\\g\",File,1,1,0,0,\n",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    reports.push(run_target(
        "csv-import",
        iterations,
        &mut rng,
        csv_seeds.clone(),
        4096,
        |input| {
            let _ = import::read_csv(input);
            let _ = import::read_csv_with_local_time(input, Ok);
        },
    ));
    let tsv_seeds: Vec<Vec<u8>> = csv_seeds
        .iter()
        .map(|s| {
            s.iter()
                .map(|&b| if b == b',' { b'\t' } else { b })
                .collect()
        })
        .collect();
    reports.push(run_target(
        "tsv-import",
        iterations,
        &mut rng,
        tsv_seeds,
        4096,
        |input| {
            let _ = import::read_tsv_with_timezone(input, Ok, import::CsvTimeZone::Utc);
        },
    ));
    let mut json = Vec::new();
    winblaze_index::export::write_json(&sample_tree(), &mut json).unwrap();
    reports.push(run_target(
        "json-import",
        iterations,
        &mut rng,
        vec![json, b"{}".to_vec(), b"[]".to_vec()],
        8192,
        |input| {
            let _ = import::read_json(input);
        },
    ));

    let mft_seed = {
        // A plausible FILE record header followed by attribute-ish bytes.
        let mut record = vec![0u8; 1024];
        record[..4].copy_from_slice(b"FILE");
        record[4..6].copy_from_slice(&48u16.to_le_bytes()); // update sequence offset
        record[6..8].copy_from_slice(&3u16.to_le_bytes()); // update sequence count
        record[20..22].copy_from_slice(&56u16.to_le_bytes()); // first attribute
        record[22..24].copy_from_slice(&1u16.to_le_bytes()); // in use
        record[24..28].copy_from_slice(&1024u32.to_le_bytes());
        record[28..32].copy_from_slice(&1024u32.to_le_bytes());
        record[56..60].copy_from_slice(&0x30u32.to_le_bytes()); // $FILE_NAME
        record[60..64].copy_from_slice(&104u32.to_le_bytes());
        record[160..164].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        record
    };
    let root = Path::new("Z:\\");
    reports.push(run_target(
        "mft-records",
        iterations,
        &mut rng,
        vec![
            mft_seed.clone(),
            [mft_seed.clone(), mft_seed.clone()].concat(),
        ],
        8192,
        |input| {
            let _ = winblaze_scanner::ntfs::parse_mft_records(root, input);
        },
    ));
    reports.push(run_target(
        "mft-dump",
        iterations,
        &mut rng,
        vec![mft_seed.clone()],
        8192,
        |input| {
            let _ = winblaze_scanner::ntfs::read_mft_dump(root, Cursor::new(input));
        },
    ));

    let snapshot_dir = scratch.join("snapshot");
    let snapshot = snapshot_seed(&snapshot_dir);
    let snapshot_file = std::fs::read_dir(&snapshot_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "bin"))
        .unwrap();
    reports.push(run_target(
        "snapshot-cache",
        iterations / 4,
        &mut rng,
        vec![snapshot],
        1 << 16,
        |input| {
            std::fs::write(&snapshot_file, input).unwrap();
            let repository = SqliteIndexRepository::open(&snapshot_dir, IndexBackend::BinaryCache);
            let _ = TreeIndex::build(repository.into_transaction());
        },
    ));

    let hint_path = scratch.join("capacity-hint.bin");
    winblaze_native::capacity_hint::save_hint(&hint_path, "Z:\\\0scope", 1000, 100).unwrap();
    let hint = std::fs::read(&hint_path).unwrap();
    reports.push(run_target(
        "capacity-hint",
        iterations,
        &mut rng,
        vec![hint],
        4096,
        |input| {
            std::fs::write(&hint_path, input).unwrap();
            let _ = winblaze_native::capacity_hint::load_hint(&hint_path, "Z:\\\0scope");
        },
    ));

    let update_seeds = vec![
        br#"{"tag_name":"v0.9.2","assets":[]}"#.to_vec(),
        br#"{"files":[{"kind":"portable","sha256":"ABCDEF0123456789abcdef0123456789ABCDEF0123456789abcdef0123456789"}]}"#.to_vec(),
        b"0.9.1".to_vec(),
    ];
    reports.push(run_target(
        "update-manifest",
        iterations,
        &mut rng,
        update_seeds,
        2048,
        |input| {
            let text = String::from_utf8_lossy(input);
            let _ = winblaze_core::parse_release_tag(&text);
            let _ = winblaze_core::parse_manifest_sha256(&text, "portable");
            let mut middle = text.len() / 2;
            while !text.is_char_boundary(middle) {
                middle -= 1;
            }
            let (a, b) = text.split_at(middle);
            let _ = winblaze_core::is_newer(a, b);
            let _ = winblaze_core::hash_matches(a, b);
        },
    ));

    let tree = sample_tree();
    let pattern_seeds = vec![
        b"a".to_vec(),
        b"*.txt".to_vec(),
        b"(a|b)+[0-9]{2,}".to_vec(),
        b"\xc3\xa9t".to_vec(),
        b"sub\\B".to_vec(),
    ];
    reports.push(run_target(
        "search-and-filters",
        iterations,
        &mut rng,
        pattern_seeds,
        256,
        |input| {
            let text = String::from_utf8_lossy(input).into_owned();
            for (i, mode) in [
                TextMode::Contains,
                TextMode::Exact,
                TextMode::Prefix,
                TextMode::Regex,
            ]
            .into_iter()
            .enumerate()
            {
                let options = SearchOptions {
                    pattern: text.clone(),
                    mode,
                    extensions: vec![text.clone()],
                    descending: i % 2 == 0,
                    offset: i,
                    ..SearchOptions::default()
                };
                let _ = tree_search::search(&tree, &options);
            }
            let filter = ExportFilter {
                patterns: text.split('|').map(str::to_owned).collect(),
                min_bytes: None,
                max_bytes: None,
                modified_after_utc: None,
                modified_before_utc: None,
            };
            let mut out = Vec::new();
            let _ = winblaze_index::export::write_csv_filtered(&tree, &filter, &mut out);
        },
    ));

    let _ = std::fs::remove_dir_all(&scratch);
    println!(
        "{:<20} {:>9} {:>7} {:>6} {:>12}",
        "target", "runs", "panics", "slow", "max time"
    );
    let mut failed = false;
    for r in &reports {
        failed |= r.panics + r.slow > 0;
        println!(
            "{:<20} {:>9} {:>7} {:>6} {:>12?}",
            r.target, r.runs, r.panics, r.slow, r.max
        );
    }
    std::process::exit(i32::from(failed));
}
