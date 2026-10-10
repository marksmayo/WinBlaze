//! Shipped unattended scan and analysis interface.
use std::time::{Duration, Instant};
#[cfg(test)]
use std::{ffi::c_void, sync::mpsc::SyncSender};
use winblaze_native::api::WbCStringView;
#[cfg(test)]
use winblaze_native::api::{WbEvent, WbEventKind};
use winblaze_native::bridge::{run_terminal_scan, wb_dump_mft, wb_export_analysis, wb_export_csv};

#[cfg(test)]
struct State {
    terminal: SyncSender<bool>,
}
#[cfg(test)]
extern "C" fn event(event: *const WbEvent, context: *mut c_void) {
    // SAFETY: the session invokes this callback with live pointers supplied below.
    let (event, state) = unsafe { (&*event, &*(context.cast::<State>())) };
    match event.kind {
        WbEventKind::Completed => {
            let _ = state.terminal.try_send(true);
        }
        WbEventKind::Failed | WbEventKind::Cancelled => {
            let _ = state.terminal.try_send(false);
        }
        _ => {}
    }
}
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, root, output) = match args.as_slice() {
        [_, root, output] => ("--scan", root, output),
        [_, mode, root, output]
            if matches!(
                mode.as_str(),
                "--scan" | "--duplicates" | "--permissions" | "--dump-mft"
            ) =>
        {
            (mode.as_str(), root, output)
        }
        _ => {
            eprintln!("usage: winblaze-cli [--scan|--duplicates|--permissions|--dump-mft] <root> <output.csv>");
            std::process::exit(2);
        }
    };
    if mode == "--dump-mft" {
        let result = wb_dump_mft(view(root), view(output));
        if result != 0 {
            eprintln!("MFT export failed (code {result}); administrator access is required for raw volume reads");
            std::process::exit(1);
        }
        return;
    }
    let phase_path = std::env::var_os("WINBLAZE_CLI_PHASES");
    let started = Instant::now();
    let outcome = run_terminal_scan(root, Duration::from_secs(300), None, std::ptr::null_mut());
    let session_started_us = phase_path
        .as_ref()
        .map(|_| outcome.session_setup.as_micros());
    let terminal_received_us = phase_path
        .as_ref()
        .map(|_| outcome.terminal_received.as_micros());
    let finalized_us = phase_path.as_ref().map(|_| started.elapsed().as_micros());
    if !outcome.completed {
        eprintln!("scan failed or timed out");
        std::process::exit(1);
    }
    let result = match mode {
        "--duplicates" => wb_export_analysis(view(output), 1),
        "--permissions" => wb_export_analysis(view(output), 2),
        _ => wb_export_csv(view(output)),
    };
    if result != 0 {
        eprintln!("CSV export failed");
        std::process::exit(1);
    }
    if let Some(path) = phase_path {
        let exported_us = started.elapsed().as_micros();
        let phases = format!(
            "{{\"session_started_us\":{},\"terminal_received_us\":{},\"finalized_us\":{},\"exported_us\":{exported_us}}}",
            session_started_us.unwrap(), terminal_received_us.unwrap(), finalized_us.unwrap()
        );
        if let Err(error) = std::fs::write(path, phases) {
            eprintln!("Could not write optional CLI phase diagnostic: {error}");
        }
    }
    println!("{{\"scan_export_ms\":{}}}", started.elapsed().as_millis());
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_terminal_callback_notifies_waiter_without_polling() {
        for (kind, success) in [
            (WbEventKind::Completed, true),
            (WbEventKind::Failed, false),
            (WbEventKind::Cancelled, false),
        ] {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let state = State { terminal: tx };
            let progress = WbEvent {
                kind: WbEventKind::Progress,
                ..Default::default()
            };
            event(&progress, (&state as *const State).cast_mut().cast());
            assert!(rx.try_recv().is_err());
            let terminal = WbEvent {
                kind,
                ..Default::default()
            };
            event(&terminal, (&state as *const State).cast_mut().cast());
            assert_eq!(rx.recv_timeout(Duration::ZERO).unwrap(), success);
            event(&terminal, (&state as *const State).cast_mut().cast());
            event(&terminal, (&state as *const State).cast_mut().cast());
            assert_eq!(rx.try_recv().unwrap(), success);
        }
    }
}
