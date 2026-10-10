#[test]
fn cli_phase_diagnostic_reports_complete_scan_finalization_and_export() {
    let root = std::env::temp_dir().join(format!("winblaze-cli-phases-{}", std::process::id()));
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("empty.bin"), []).unwrap();
    let phases = root.join("phases.json");
    let csv = root.join("scan.csv");
    let _ = std::fs::remove_file(&phases);
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_winblaze-cli"))
        .arg(&data)
        .arg(&csv)
        .env("LOCALAPPDATA", root.join("cache"))
        .env("WINBLAZE_CLI_PHASES", &phases)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(std::fs::read_to_string(csv).unwrap().contains("empty.bin"));
    let phases: serde_json::Value = serde_json::from_slice(
        &std::fs::read(phases).expect("opt-in complete CLI phase diagnostic missing"),
    )
    .unwrap();
    let mut previous = 0;
    for key in [
        "session_started_us",
        "terminal_received_us",
        "finalized_us",
        "exported_us",
    ] {
        let current = phases[key].as_u64().expect("phase timestamp missing");
        assert!(current >= previous, "unordered phase {key}");
        previous = current;
    }
}

#[test]
fn shipped_cli_scans_exports_and_rejects_invalid_commands() {
    let root = std::env::temp_dir().join(format!("winblaze-cli-{}", std::process::id()));
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("a"), b"same").unwrap();
    std::fs::write(data.join("b"), b"same").unwrap();
    let cli = env!("CARGO_BIN_EXE_winblaze-cli");
    for (mode, name, header) in [
        ("--scan", "scan.csv", "Path,Kind"),
        ("--duplicates", "duplicates.csv", "Group,Path"),
        ("--permissions", "permissions.csv", "Path,SDDL"),
    ] {
        let result = std::process::Command::new(cli)
            .args([
                mode,
                data.to_str().unwrap(),
                root.join(name).to_str().unwrap(),
            ])
            .env("LOCALAPPDATA", root.join("cache"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(std::fs::read_to_string(root.join(name))
            .unwrap()
            .starts_with(header));
    }
    assert!(!std::process::Command::new(cli)
        .arg("--unknown")
        .status()
        .unwrap()
        .success());
}
