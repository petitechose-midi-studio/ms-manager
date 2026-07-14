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
    #[serde(default)]
    pub compatibility: StepPresetCompatibility,
    #[serde(default)]
    pub format_version: u8,
    #[serde(default)]
    pub technical_id: String,
    #[serde(default)]
    pub semantic_name: String,
    #[serde(default)]
    pub metadata_defaulted: bool,
    #[serde(default)]
    pub mixed_pitch_policy: bool,
    #[serde(default)]
    pub scale_policy: StepPresetScalePolicy,
    #[serde(default)]
    pub default_scale_policy: StepPresetScalePolicy,
    #[serde(default)]
    pub source_scale: StepPresetSourceScale,
    pub root_context: bool,
    pub root_values: bool,
    pub step_node_count: u16,
    pub sequence_count: u8,
    pub cycle_set_count: u8,
    #[serde(default)]
    pub bytes_written: u32,
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
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepPresetCompatibility {
    Ready,
    ReadyMixed,
    WarningLegacyDefaulted,
    UnsupportedVersion,
    BlockedInvalid,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepPresetScalePolicy {
    Chromatic,
    ScaleRelative,
    Mixed,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPresetSourceScale {
    pub root: u8,
    #[serde(rename = "type")]
    pub scale_type: u8,
    pub mode: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPresetFlags {
    pub root_values: bool,
    pub graph_payload: bool,
    pub overwrite: bool,
    #[serde(default)]
    pub mixed_pitch_policy: bool,
}

#[derive(Debug, Error)]
pub enum StepPresetError {
    #[error("step preset tool failed to start: {0}")]
    Spawn(std::io::Error),
    #[error("step preset tool returned no JSON output")]
    EmptyOutput,
    #[error("step preset tool returned invalid JSON: {0}")]
    Json(serde_json::Error),
    #[error("step preset tool failed: {status:?}: {stderr}")]
    ToolFailed { status: Option<i32>, stderr: String },
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

    pub fn rename(
        &self,
        input: &Path,
        semantic_name: &str,
        output: &Path,
    ) -> Result<StepPresetReport, StepPresetError> {
        self.run([
            "rename-step-graph-preset".as_ref(),
            input.as_os_str(),
            "--name".as_ref(),
            semantic_name.as_ref(),
            "--out".as_ref(),
            output.as_os_str(),
            "--json".as_ref(),
        ])
    }

    fn run<I, S>(&self, args: I) -> Result<StepPresetReport, StepPresetError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = run_core_file_tool(&self.tool_path, args).map_err(StepPresetError::Spawn)?;
        let report = parse_step_preset_report(&output.stdout)?;
        // Invalid assets intentionally return a structured non-OK report with
        // a non-zero process status. A non-zero exit paired with an OK report,
        // however, means the tool failed after producing the report (for
        // example while writing a staged rename) and must never be actionable.
        if !output.success && report.status == StepPresetStatus::Ok {
            return Err(StepPresetError::ToolFailed {
                status: output.status,
                stderr: output.stderr,
            });
        }
        Ok(report)
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
            r#"{"operation":"validate-step-graph-preset","fileKind":"step_graph_preset","status":"ok","compatibility":"ready_mixed","formatVersion":2,"technicalId":"human-groove","semanticName":"Human groove","metadataDefaulted":false,"mixedPitchPolicy":true,"scalePolicy":"mixed","defaultScalePolicy":"scale_relative","sourceScale":{"root":2,"type":1,"mode":1},"rootContext":true,"rootValues":true,"stepNodeCount":3,"sequenceCount":1,"cycleSetCount":0,"bytesWritten":0,"flags":{"rootValues":true,"graphPayload":true,"overwrite":false,"mixedPitchPolicy":true}}"#,
        )
        .unwrap();

        assert_eq!(report.operation, "validate-step-graph-preset");
        assert_eq!(report.file_kind, "step_graph_preset");
        assert_eq!(report.status, StepPresetStatus::Ok);
        assert_eq!(report.compatibility, StepPresetCompatibility::ReadyMixed);
        assert_eq!(report.format_version, 2);
        assert_eq!(report.technical_id, "human-groove");
        assert_eq!(report.semantic_name, "Human groove");
        assert!(report.mixed_pitch_policy);
        assert_eq!(report.scale_policy, StepPresetScalePolicy::Mixed);
        assert_eq!(
            report.default_scale_policy,
            StepPresetScalePolicy::ScaleRelative
        );
        assert_eq!(report.source_scale.root, 2);
        assert_eq!(report.source_scale.scale_type, 1);
        assert_eq!(report.source_scale.mode, 1);
        assert!(report.root_context);
        assert!(report.root_values);
        assert_eq!(report.step_node_count, 3);
        assert_eq!(report.sequence_count, 1);
        assert_eq!(report.cycle_set_count, 0);
        assert!(report.flags.root_values);
        assert!(report.flags.graph_payload);
        assert!(!report.flags.overwrite);
        assert!(report.flags.mixed_pitch_policy);
    }

    #[test]
    fn parses_legacy_tool_report_with_explicit_unknown_metadata() {
        let report = parse_step_preset_report(
            r#"{"operation":"inspect-step-graph-preset","fileKind":"step_graph_preset","status":"ok","rootContext":true,"rootValues":false,"stepNodeCount":1,"sequenceCount":0,"cycleSetCount":0,"flags":{"rootValues":false,"graphPayload":true,"overwrite":false}}"#,
        )
        .unwrap();

        assert_eq!(report.compatibility, StepPresetCompatibility::Unknown);
        assert_eq!(report.scale_policy, StepPresetScalePolicy::Unknown);
        assert_eq!(report.format_version, 0);
        assert!(report.technical_id.is_empty());
        assert!(report.semantic_name.is_empty());
    }

    #[test]
    fn rejects_empty_step_preset_report() {
        assert!(matches!(
            parse_step_preset_report(""),
            Err(StepPresetError::EmptyOutput)
        ));
    }

    #[test]
    fn maps_future_tool_status_to_unknown_instead_of_failing_json() {
        let report = parse_step_preset_report(
            r#"{"operation":"inspect-step-graph-preset","fileKind":"step_graph_preset","status":"future_status","rootContext":false,"rootValues":false,"stepNodeCount":0,"sequenceCount":0,"cycleSetCount":0,"flags":{"rootValues":false,"graphPayload":false,"overwrite":false}}"#,
        )
        .unwrap();

        assert_eq!(report.status, StepPresetStatus::Unknown);
    }
}
