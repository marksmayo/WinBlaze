use winblaze_native::{api::WbCStringView, bridge::wb_dump_mft};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
#[test]
fn invalid_volume_does_not_overwrite_an_existing_destination() {
    let output =
        std::env::temp_dir().join(format!("winblaze-mft-invalid-{}.bin", std::process::id()));
    std::fs::write(&output, b"keep existing report").unwrap();
    assert_ne!(
        wb_dump_mft(
            view("C:\\not-a-volume-root"),
            view(&output.to_string_lossy())
        ),
        0
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing report");
    std::fs::remove_file(output).unwrap();
    assert_eq!(wb_dump_mft(view("C:\\"), view("bad\0path")), 1);
}
