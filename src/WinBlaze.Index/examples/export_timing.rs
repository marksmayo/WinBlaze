//! Times the complete CSV export and the snapshot write of a saved catalog,
//! separately and overlapped, as the CLI runs them after a scan.
//! Usage: export_timing <index directory> <scratch directory>
use std::time::Instant;

use winblaze_index::{IndexBackend, IndexRepository, SqliteIndexRepository, TreeIndex};

fn main() {
    let mut args = std::env::args().skip(1);
    let index = std::path::PathBuf::from(args.next().expect("index directory"));
    let scratch = std::path::PathBuf::from(args.next().expect("scratch directory"));
    let started = Instant::now();
    let repository = SqliteIndexRepository::open(&index, IndexBackend::BinaryCache);
    let tree = TreeIndex::build(repository.into_transaction());
    println!(
        "load+build {:?}: {} directories, {} files",
        started.elapsed(),
        tree.directories().len(),
        tree.files().len()
    );
    let export = |tree: &TreeIndex| {
        let started = Instant::now();
        let mut sink = std::io::BufWriter::with_capacity(4 << 20, std::io::sink());
        winblaze_index::export::write_csv(tree, &mut sink).unwrap();
        started.elapsed()
    };
    let persist = |tree: &TreeIndex| {
        let started = Instant::now();
        let target = SqliteIndexRepository::open_empty(&scratch, IndexBackend::BinaryCache);
        target
            .persist_sorted_records(
                tree.volumes(),
                &[],
                tree.directories(),
                &mut {
                    let mut cache = winblaze_index::tree::DirectoryPathCache::default();
                    move |directory, out| {
                        tree.push_directory_path_cached(directory, &mut cache, out)
                    }
                },
                tree.files(),
                &[],
                &[],
            )
            .unwrap();
        started.elapsed()
    };
    println!("export alone {:?}", export(&tree));
    println!("persist alone {:?}", persist(&tree));
    let (exported, persisted) = std::thread::scope(|scope| {
        let writer = scope.spawn(|| persist(&tree));
        let exported = export(&tree);
        (exported, writer.join().unwrap())
    });
    println!("overlapped: export {exported:?}, persist {persisted:?}");
    let started = Instant::now();
    let mut path = String::new();
    for directory in tree.directories() {
        path.clear();
        tree.push_directory_path(directory, &mut path);
    }
    println!("derive every directory path {:?}", started.elapsed());
    let started = Instant::now();
    let mut cache = winblaze_index::tree::DirectoryPathCache::default();
    for directory in tree.directories() {
        path.clear();
        tree.push_directory_path_cached(directory, &mut cache, &mut path);
    }
    println!(
        "derive every directory path, cached {:?}",
        started.elapsed()
    );
}
