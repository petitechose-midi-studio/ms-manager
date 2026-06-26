use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core_file_tool::run_core_file_tool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPresetReport {
    pub operation: String,
    pub file_kind: String,
    pub status: StepPresetStatus,
    pub root_context: bool,
    pub root_values: bool,
    pub step_node_count: u16,
    pub sequence_count: u8,
    pub cycle_set_count: u8,
    pub flags: StepPresetFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepPresetStatus {
    Ok,
    InvalidArgument,
    InvalidFormat,
    UnsupportedVersion,
    IncompatibleTarget,
    GraphLimitReached,
    BufferTooSmall,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPresetFlags {
    pub root_values: bool,
    pub graph_payload: bool,
    pub overwrite: bool,
}

#[derive(Debug, Error)]
pub enum StepPresetError {
    #[error("step preset tool failed to start: {0}")]
    Spawn(std::io::Error),
    #[error("step preset tool returned no JSON output")]
    EmptyOutput,
    #[error("step preset tool returned invalid JSON: {0}")]
    Json(serde_json::Error),
}

pub struct StepPresetTool {
    tool_path: PathBuf,
}

impl StepPresetTool {
    pub fn new(tool_path: impl Into<PathBuf>) -> Self {
        Self {
            tool_path: tool_path.into(),
        }
    }

    pub fn inspect(&self, input: &Path) -> Result<StepPresetReport, StepPresetError> {
        self.run([
            "inspect-step-graph-preset".as_ref(),
            input.as_os_str(),
            "--json".as_ref(),
        ])
    }

    pub fn validate(&self, input: &Path) -> Result<StepPresetReport, StepPresetError> {
        self.run([
            "validate-step-graph-preset".as_ref(),
            input.as_os_str(),
            "--json".as_ref(),
        ])
    }

    fn run<I, S>(&self, args: I) -> Result<StepPresetReport, StepPresetError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = run_core_file_tool(&self.tool_path, args).map_err(StepPresetError::Spawn)?;
        parse_step_preset_report(&output.stdout)
    }
}

pub fn parse_step_preset_report(json: &str) -> Result<StepPresetReport, StepPresetError> {
    if json.trim().is_empty() {
        return Err(StepPresetError::EmptyOutput);
    }
    serde_json::from_str(json).map_err(StepPresetError::Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_step_preset_report() {
        let report = parse_step_preset_report(
            r#"{"operation":"validate-step-graph-preset","fileKind":"step_graph_preset","status":"ok","rootContext":true,"rootValues":true,"stepNodeCount":3,"sequenceCount":1,"cycleSetCount":0,"flags":{"rootValues":true,"graphPayload":true,"overwrite":false}}"#,
        )
        .unwrap();

        assert_eq!(report.operation, "validate-step-graph-preset");
        assert_eq!(report.file_kind, "step_graph_preset");
        assert_eq!(report.status, StepPresetStatus::Ok);
        assert!(report.root_context);
        assert!(report.root_values);
        assert_eq!(report.step_node_count, 3);
        assert_eq!(report.sequence_count, 1);
        assert_eq!(report.cycle_set_count, 0);
        assert!(report.flags.root_values);
        assert!(report.flags.graph_payload);
        assert!(!report.flags.overwrite);
    }

    #[test]
    fn rejects_empty_step_preset_report() {
        assert!(matches!(
            parse_step_preset_report(""),
            Err(StepPresetError::EmptyOutput)
        ));
    }
}
