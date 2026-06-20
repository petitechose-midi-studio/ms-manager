use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMigrationReport {
    pub operation: String,
    pub file_kind: String,
    pub status: ProjectMigrationStatus,
    pub load_status: ProjectLoadStatus,
    pub container_status: String,
    pub overwrite_safe: bool,
    pub has_unknown_unsupported_data: bool,
    pub bytes_written: u32,
    pub items: Vec<ProjectLoadReportItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMigrationStatus {
    Current,
    Migrated,
    Partial,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLoadStatus {
    Ok,
    Migrated,
    Partial,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLoadReportItem {
    pub severity: String,
    pub code: String,
    pub chunk_id: u32,
    pub source_major: u8,
    pub source_minor: u8,
    pub target_major: u8,
    pub target_minor: u8,
}

#[derive(Debug, Error)]
pub enum ProjectMigrationError {
    #[error("project migration tool failed to start: {0}")]
    Spawn(std::io::Error),
    #[error("project migration tool returned no JSON output")]
    EmptyOutput,
    #[error("project migration tool returned invalid JSON: {0}")]
    Json(serde_json::Error),
    #[error("project migration tool failed: {status:?}: {stderr}")]
    ToolFailed {
        status: Option<i32>,
        stderr: String,
    },
}

pub struct ProjectMigrationTool {
    tool_path: PathBuf,
}

impl ProjectMigrationTool {
    pub fn new(tool_path: impl Into<PathBuf>) -> Self {
        Self {
            tool_path: tool_path.into(),
        }
    }

    pub fn inspect(&self, input: &Path) -> Result<ProjectMigrationReport, ProjectMigrationError> {
        self.run(["inspect".as_ref(), input.as_os_str(), "--json".as_ref()])
    }

    pub fn validate(&self, input: &Path) -> Result<ProjectMigrationReport, ProjectMigrationError> {
        self.run(["validate".as_ref(), input.as_os_str(), "--json".as_ref()])
    }

    pub fn migrate(
        &self,
        input: &Path,
        output: &Path,
        allow_partial: bool,
    ) -> Result<ProjectMigrationReport, ProjectMigrationError> {
        let mut args = vec![
            "migrate".into(),
            input.as_os_str().to_owned(),
            "--out".into(),
            output.as_os_str().to_owned(),
            "--json".into(),
        ];
        if allow_partial {
            args.push("--allow-partial".into());
        }
        self.run(args)
    }

    fn run<I, S>(&self, args: I) -> Result<ProjectMigrationReport, ProjectMigrationError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new(&self.tool_path)
            .args(args)
            .output()
            .map_err(ProjectMigrationError::Spawn)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let report = parse_project_migration_report(stdout.trim())?;

        if !output.status.success()
            && !matches!(report.status, ProjectMigrationStatus::Partial)
        {
            return Err(ProjectMigrationError::ToolFailed {
                status: output.status.code(),
                stderr,
            });
        }

        Ok(report)
    }
}

pub fn parse_project_migration_report(
    json: &str,
) -> Result<ProjectMigrationReport, ProjectMigrationError> {
    if json.trim().is_empty() {
        return Err(ProjectMigrationError::EmptyOutput);
    }
    serde_json::from_str(json).map_err(ProjectMigrationError::Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_migration_report() {
        let report = parse_project_migration_report(
            r#"{"operation":"inspect","fileKind":"project","status":"partial","loadStatus":"partial","containerStatus":"ok","overwriteSafe":false,"hasUnknownUnsupportedData":true,"bytesWritten":0,"items":[{"severity":"warning","code":"unsupported_chunk_version","chunkId":1397051730,"sourceMajor":1,"sourceMinor":0,"targetMajor":1,"targetMinor":1}]}"#,
        )
        .unwrap();

        assert_eq!(report.operation, "inspect");
        assert_eq!(report.file_kind, "project");
        assert_eq!(report.status, ProjectMigrationStatus::Partial);
        assert_eq!(report.load_status, ProjectLoadStatus::Partial);
        assert!(!report.overwrite_safe);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].code, "unsupported_chunk_version");
    }
}
