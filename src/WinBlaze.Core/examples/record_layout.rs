//! Reports actual record representation sizes; not a process-memory benchmark.
use std::mem::size_of;
use winblaze_core::{DirectoryRecord, FileRecord};

fn main() {
    println!(
        "{{\"file_record_bytes\":{},\"directory_record_bytes\":{},\"string_bytes\":{},\"boxed_str_bytes\":{},\"optional_i64_bytes\":{},\"i64_bytes\":{}}}",
        size_of::<FileRecord>(),
        size_of::<DirectoryRecord>(),
        size_of::<String>(),
        size_of::<Box<str>>(),
        size_of::<Option<i64>>(),
        size_of::<i64>(),
    );
}
