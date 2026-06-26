use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

pub(crate) struct CoreFileToolOutput {
    pub success: bool,
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn run_core_file_tool<I, S>(
    tool_path: &Path,
    args: I,
) -> Result<CoreFileToolOutput, std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(tool_path).args(args).output()?;

    Ok(CoreFileToolOutput {
        success: output.status.success(),
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}
