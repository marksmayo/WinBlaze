use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanErrorKind {
    PermissionDenied,
    NotFound,
    SharingViolation,
    TransientIo,
    UnsupportedFilesystem,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanErrorRecord {
    pub kind: ScanErrorKind,
    pub path: Option<String>,
    pub message: String,
}

const ERROR_NOT_READY: i32 = 21;
const ERROR_DEV_NOT_EXIST: i32 = 55;
const ERROR_OPEN_FAILED: i32 = 110;
const ERROR_SEM_TIMEOUT: i32 = 121;
const ERROR_DIRECTORY: i32 = 267;
const ERROR_OPERATION_ABORTED: i32 = 995;
const ERROR_NO_MEDIA_IN_DRIVE: i32 = 1112;
const ERROR_IO_DEVICE: i32 = 1117;
const ERROR_DEVICE_NOT_CONNECTED: i32 = 1167;
const ERROR_REQUEST_ABORTED: i32 = 1235;

pub fn classify_io_error(error: &io::Error) -> ScanErrorKind {
    if let Some(code) = error.raw_os_error() {
        return match code {
            2 | 3 | ERROR_DIRECTORY => ScanErrorKind::NotFound,
            5 => ScanErrorKind::PermissionDenied,
            32 | 33 => ScanErrorKind::SharingViolation,
            ERROR_NOT_READY
            | ERROR_DEV_NOT_EXIST
            | ERROR_OPEN_FAILED
            | 116
            | ERROR_SEM_TIMEOUT
            | ERROR_NO_MEDIA_IN_DRIVE
            | ERROR_IO_DEVICE
            | ERROR_DEVICE_NOT_CONNECTED
            | ERROR_OPERATION_ABORTED
            | ERROR_REQUEST_ABORTED => ScanErrorKind::TransientIo,
            _ => classify_io_error_kind(error.kind()),
        };
    }

    classify_io_error_kind(error.kind())
}

fn classify_io_error_kind(kind: io::ErrorKind) -> ScanErrorKind {
    match kind {
        io::ErrorKind::PermissionDenied => ScanErrorKind::PermissionDenied,
        io::ErrorKind::NotFound => ScanErrorKind::NotFound,
        io::ErrorKind::WouldBlock
        | io::ErrorKind::Interrupted
        | io::ErrorKind::TimedOut
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::WriteZero => ScanErrorKind::TransientIo,
        _ => ScanErrorKind::Unknown,
    }
}

pub fn is_permission_failure(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::PermissionDenied)
}

pub fn is_transient_io_error(error: &io::Error) -> bool {
    classify_io_error(error) == ScanErrorKind::TransientIo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_io_errors_into_expected_buckets() {
        assert_eq!(
            classify_io_error(&io::Error::from(io::ErrorKind::PermissionDenied)),
            ScanErrorKind::PermissionDenied
        );
        assert_eq!(
            classify_io_error(&io::Error::from_raw_os_error(32)),
            ScanErrorKind::SharingViolation
        );
        assert_eq!(
            classify_io_error(&io::Error::from_raw_os_error(3)),
            ScanErrorKind::NotFound
        );
        assert_eq!(
            classify_io_error(&io::Error::from_raw_os_error(ERROR_DIRECTORY)),
            ScanErrorKind::NotFound
        );
        assert_eq!(
            classify_io_error(&io::Error::from_raw_os_error(21)),
            ScanErrorKind::TransientIo
        );
        assert_eq!(
            classify_io_error(&io::Error::from_raw_os_error(1117)),
            ScanErrorKind::TransientIo
        );
        assert_eq!(
            classify_io_error(&io::Error::from(io::ErrorKind::TimedOut)),
            ScanErrorKind::TransientIo
        );
        assert!(is_permission_failure(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(is_transient_io_error(&io::Error::from(
            io::ErrorKind::Interrupted
        )));
    }

    #[test]
    fn classifies_removable_disconnect_codes_as_transient() {
        for code in [
            ERROR_NOT_READY,
            ERROR_DEV_NOT_EXIST,
            ERROR_SEM_TIMEOUT,
            ERROR_OPERATION_ABORTED,
            ERROR_NO_MEDIA_IN_DRIVE,
            ERROR_IO_DEVICE,
            ERROR_DEVICE_NOT_CONNECTED,
            ERROR_REQUEST_ABORTED,
        ] {
            let error = io::Error::from_raw_os_error(code);
            assert_eq!(
                classify_io_error(&error),
                ScanErrorKind::TransientIo,
                "code {code} should be transient"
            );
            assert!(
                is_transient_io_error(&error),
                "code {code} should satisfy transient helper"
            );
        }
    }
}
