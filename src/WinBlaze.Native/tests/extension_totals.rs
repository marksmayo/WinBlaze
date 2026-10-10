use std::ffi::c_void;
use winblaze_native::{
    api::{WbCStringView, WbExtensionStat},
    bridge::{wb_import_csv, wb_index_snapshot_extension_stats},
};
extern "C" fn stat(row: *const WbExtensionStat, out: *mut c_void) {
    unsafe {
        let counts = &mut *out.cast::<(u64, u64, u64)>();
        counts.0 += 1;
        counts.1 += (*row).files;
        counts.2 += (*row).bytes;
    }
}
#[test]
fn extension_totals_include_every_extension_not_only_the_live_top_forty() {
    let path = std::env::temp_dir().join(format!("winblaze-extension-{}.csv", std::process::id()));
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for i in 0..43 {
        csv.push_str(&format!("Z:\\offline\\file.ext{i},File,7,4096,0,0,\n"));
    }
    std::fs::write(&path, csv).unwrap();
    let text = path.to_str().unwrap();
    assert_eq!(
        wb_import_csv(WbCStringView {
            ptr: text.as_ptr().cast(),
            len: text.len()
        }),
        0
    );
    let mut totals = (0_u64, 0_u64, 0_u64);
    wb_index_snapshot_extension_stats(Some(stat), (&mut totals as *mut (u64, u64, u64)).cast());
    assert_eq!(totals, (43, 43, 301));
    std::fs::remove_file(path).unwrap();
}
