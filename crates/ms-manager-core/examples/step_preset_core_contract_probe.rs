use std::path::Path;
use std::process::ExitCode;

use ms_manager_core::StepPresetTool;

fn usage() -> ExitCode {
    eprintln!(
        "usage: step_preset_core_contract_probe <core-file-tool> <inspect|validate> <fixture.mssp>\n       step_preset_core_contract_probe <core-file-tool> rename <fixture.mssp> <semantic-name> <output.mssp>"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(tool_path) = args.next() else {
        return usage();
    };
    let Some(operation) = args.next() else {
        return usage();
    };
    let Some(fixture) = args.next() else {
        return usage();
    };
    let tool = StepPresetTool::new(tool_path);
    let report = match operation.to_str() {
        Some("inspect") if args.next().is_none() => tool.inspect(Path::new(&fixture)),
        Some("validate") if args.next().is_none() => tool.validate(Path::new(&fixture)),
        Some("rename") => {
            let Some(semantic_name) = args.next() else {
                return usage();
            };
            let Some(output) = args.next() else {
                return usage();
            };
            if args.next().is_some() {
                return usage();
            }
            let Some(semantic_name) = semantic_name.to_str() else {
                eprintln!("semantic name is not valid UTF-8");
                return usage();
            };
            tool.rename(Path::new(&fixture), semantic_name, Path::new(&output))
        }
        Some("inspect" | "validate") => return usage(),
        Some(other) => {
            eprintln!("unsupported probe operation: {other}");
            return usage();
        }
        None => {
            eprintln!("probe operation is not valid UTF-8");
            return usage();
        }
    };
    let report = match report {
        Ok(report) => report,
        Err(error) => {
            eprintln!("manager StepPresetTool failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = serde_json::to_writer(std::io::stdout().lock(), &report) {
        eprintln!("could not serialize manager Step Preset report: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
