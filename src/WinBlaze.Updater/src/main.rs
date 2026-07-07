//! WinBlaze portable updater helper.
//!
//! A running portable app cannot overwrite its own exe/DLLs, so the app
//! downloads + extracts the new build to a staging directory, then launches
//! this tiny helper and exits. The helper waits for the app to exit, copies
//! the staged files over the install directory (retrying briefly while files
//! release their locks), relaunches the app, and cleans up.
//!
//! Usage:
//!   winblaze-updater --pid <pid> --source <staging_dir> --target <install_dir>
//!                    --relaunch <exe_path> [--cleanup <extra_dir_or_file> ...]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, io, thread};

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    pid: Option<u32>,
    source: PathBuf,
    target: PathBuf,
    relaunch: Option<PathBuf>,
    cleanup: Vec<PathBuf>,
}

fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Args, String> {
    let mut parsed = Args::default();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--pid" => parsed.pid = args.next().and_then(|value| value.parse().ok()),
            "--source" => {
                parsed.source = PathBuf::from(args.next().ok_or("missing value for --source")?)
            }
            "--target" => {
                parsed.target = PathBuf::from(args.next().ok_or("missing value for --target")?)
            }
            "--relaunch" => {
                parsed.relaunch = Some(PathBuf::from(
                    args.next().ok_or("missing value for --relaunch")?,
                ))
            }
            "--cleanup" => parsed.cleanup.push(PathBuf::from(
                args.next().ok_or("missing value for --cleanup")?,
            )),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if parsed.source.as_os_str().is_empty() || parsed.target.as_os_str().is_empty() {
        return Err(String::from("--source and --target are required"));
    }
    Ok(parsed)
}

/// Recursively copies every file under `src` into `dst`, creating directories
/// as needed and overwriting existing files. Each file copy is retried a few
/// times so a briefly-locked target (AV/indexer/just-exited process) doesn't
/// abort the whole update. Returns the number of files copied.
fn copy_tree(src: &Path, dst: &Path, retries: u32) -> io::Result<u64> {
    let mut copied = 0u64;
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copied += copy_tree(&from, &to, retries)?;
        } else {
            copy_file_with_retry(&from, &to, retries)?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn copy_file_with_retry(from: &Path, to: &Path, retries: u32) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if attempt >= retries {
                    return Err(error);
                }
                attempt += 1;
                thread::sleep(Duration::from_millis(250 * u64::from(attempt)));
            }
        }
    }
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("winblaze-updater: {message}");
            std::process::exit(2);
        }
    };

    // Wait for the app to release its files (best effort; the copy retry loop
    // is the real safety net if it outlives the wait).
    if let Some(pid) = args.pid {
        wait_for_process(pid, 30_000);
    }
    // Small settle so the exiting process fully releases file handles.
    thread::sleep(Duration::from_millis(400));

    // The source may be the extracted portable folder itself or its parent
    // containing a single portable subfolder; resolve to the folder holding
    // the executables.
    let source = resolve_source(&args.source, args.relaunch.as_deref());

    match copy_tree(&source, &args.target, 8) {
        Ok(count) => eprintln!("winblaze-updater: copied {count} files"),
        Err(error) => {
            eprintln!("winblaze-updater: copy failed: {error}");
            // Still try to relaunch the (unchanged) app so the user isn't left
            // without WinBlaze.
        }
    }

    if let Some(exe) = &args.relaunch {
        let _ = std::process::Command::new(exe)
            .current_dir(&args.target)
            .spawn();
    }

    for path in &args.cleanup {
        let _ = fs::remove_dir_all(path).or_else(|_| fs::remove_file(path));
    }
}

/// If `source` doesn't directly contain the relaunch exe but has exactly one
/// subdirectory that does (the zip's `WinBlaze-...-portable/` folder), descend
/// into it. Otherwise use `source` as-is.
fn resolve_source(source: &Path, relaunch: Option<&Path>) -> PathBuf {
    let exe_name = relaunch.and_then(|p| p.file_name());
    if let Some(name) = exe_name {
        if source.join(name).exists() {
            return source.to_path_buf();
        }
        if let Ok(entries) = fs::read_dir(source) {
            let subdirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            if let [only] = subdirs.as_slice() {
                if only.join(name).exists() {
                    return only.clone();
                }
            }
        }
    }
    source.to_path_buf()
}

fn wait_for_process(pid: u32, timeout_ms: u32) {
    const SYNCHRONIZE: u32 = 0x0010_0000;
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return; // already exited
        }
        WaitForSingleObject(handle, timeout_ms);
        CloseHandle(handle);
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "winblaze-updater-test-{tag}-{:?}",
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn parse_args_reads_flags() {
        let args = parse_args(
            [
                "--pid",
                "1234",
                "--source",
                r"C:\stage",
                "--target",
                r"C:\app",
                "--relaunch",
                r"C:\app\WinBlaze.UI.exe",
                "--cleanup",
                r"C:\tmp\x.zip",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parse");
        assert_eq!(args.pid, Some(1234));
        assert_eq!(args.source, PathBuf::from(r"C:\stage"));
        assert_eq!(args.target, PathBuf::from(r"C:\app"));
        assert_eq!(
            args.relaunch,
            Some(PathBuf::from(r"C:\app\WinBlaze.UI.exe"))
        );
        assert_eq!(args.cleanup, vec![PathBuf::from(r"C:\tmp\x.zip")]);
    }

    #[test]
    fn parse_args_requires_source_and_target() {
        assert!(parse_args(["--pid", "1"].into_iter().map(String::from)).is_err());
    }

    #[test]
    fn copy_tree_replaces_files_and_recurses() {
        let root = temp_dir("copy");
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), b"new-a").unwrap();
        fs::write(src.join("sub/b.txt"), b"new-b").unwrap();
        // Pre-existing target file must be overwritten.
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("a.txt"), b"old-a").unwrap();

        let count = copy_tree(&src, &dst, 2).unwrap();
        assert_eq!(count, 2);
        assert_eq!(fs::read(dst.join("a.txt")).unwrap(), b"new-a");
        assert_eq!(fs::read(dst.join("sub/b.txt")).unwrap(), b"new-b");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_source_descends_into_single_portable_subfolder() {
        let root = temp_dir("resolve");
        let stage = root.join("stage");
        let inner = stage.join("WinBlaze-Release-x64-portable");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("WinBlaze.UI.exe"), b"exe").unwrap();

        let resolved = resolve_source(&stage, Some(Path::new(r"C:\app\WinBlaze.UI.exe")));
        assert_eq!(resolved, inner);

        // When the exe is directly in source, source is used as-is.
        fs::write(stage.join("WinBlaze.UI.exe"), b"exe").unwrap();
        assert_eq!(
            resolve_source(&stage, Some(Path::new(r"C:\app\WinBlaze.UI.exe"))),
            stage
        );
        fs::remove_dir_all(&root).ok();
    }
}
