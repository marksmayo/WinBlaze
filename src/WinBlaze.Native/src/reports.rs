//! Complete catalog analyses shared by the CLI and GUI.
use std::{
    io::{self, Write},
    path::PathBuf,
};
use winblaze_index::TreeIndex;
fn field(output: &mut impl Write, text: &str) -> io::Result<()> {
    output.write_all(b"\"")?;
    escaped_field(output, text)?;
    output.write_all(b"\"")
}
fn escaped_field(output: &mut impl Write, text: &str) -> io::Result<()> {
    let mut parts = text.as_bytes().split(|&byte| byte == b'"');
    output.write_all(parts.next().unwrap_or_default())?;
    for part in parts {
        output.write_all(b"\"\"")?;
        output.write_all(part)?;
    }
    Ok(())
}
struct EscapedWriter<'a, W>(&'a mut W);
impl<W: Write> Write for EscapedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut parts = bytes.split(|&byte| byte == b'"');
        self.0.write_all(parts.next().unwrap_or_default())?;
        for part in parts {
            self.0.write_all(b"\"\"")?;
            self.0.write_all(part)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
fn error_field(output: &mut impl Write, error: impl std::fmt::Display) -> io::Result<()> {
    output.write_all(b"\"error: ")?;
    write!(EscapedWriter(output), "{error}")?;
    output.write_all(b"\"")
}
pub fn write_analysis(tree: &TreeIndex, kind: u32, output: &mut impl Write) -> io::Result<()> {
    let paths = tree
        .files()
        .iter()
        .map(|file| PathBuf::from(tree.file_display_path(file)));
    match kind {
        1 => {
            let report = winblaze_scanner::content::find_duplicates(paths)?;
            writeln!(output, "Group,Path,LogicalBytes,Status")?;
            for (group, paths) in report.groups.iter().enumerate() {
                for path in paths {
                    write!(output, "{},", group + 1)?;
                    field(output, &path.to_string_lossy())?;
                    writeln!(output, ",{},verified", std::fs::metadata(path)?.len())?;
                }
            }
            for (path, error) in report.errors {
                write!(output, ",")?;
                field(output, &path.to_string_lossy())?;
                write!(output, ",,")?;
                error_field(output, &error)?;
                writeln!(output)?;
            }
        }
        2 => {
            writeln!(output, "Path,SDDL,Status")?;
            for path in tree
                .directories()
                .iter()
                .map(|directory| PathBuf::from(&directory.full_path))
                .chain(paths)
            {
                field(output, &path.to_string_lossy())?;
                write!(output, ",")?;
                match winblaze_scanner::permissions::security_descriptor(&path) {
                    Ok(sddl) => {
                        field(output, &sddl)?;
                        writeln!(output, ",ok")?;
                    }
                    Err(error) => {
                        write!(output, ",")?;
                        error_field(output, &error)?;
                        writeln!(output)?;
                    }
                }
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown analysis",
            ))
        }
    }
    Ok(())
}
