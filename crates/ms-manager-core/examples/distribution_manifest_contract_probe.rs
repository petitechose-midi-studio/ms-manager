use std::path::Path;
use std::process::ExitCode;

use ms_manager_core::{parse_manifest_json, select_install_set_assets};

fn usage() -> ExitCode {
    eprintln!("usage: distribution_manifest_contract_probe <manifest.json>");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(manifest_path) = args.next() else {
        return usage();
    };
    if args.next().is_some() {
        return usage();
    }

    let bytes = match std::fs::read(Path::new(&manifest_path)) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("could not read Distribution manifest: {error}");
            return ExitCode::FAILURE;
        }
    };
    let manifest = match parse_manifest_json(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("MS Manager rejected Distribution manifest: {error}");
            return ExitCode::FAILURE;
        }
    };

    for install_set in &manifest.install_sets {
        let (Some(os), Some(arch)) = (install_set.os.as_deref(), install_set.arch.as_deref())
        else {
            continue;
        };
        if let Err(error) = select_install_set_assets(&manifest, &install_set.id, os, arch) {
            eprintln!(
                "MS Manager rejected install set {} ({os}/{arch}): {error}",
                install_set.id
            );
            return ExitCode::FAILURE;
        }
    }

    println!(
        "compatible schema={} tag={} assets={} install_sets={}",
        manifest.schema,
        manifest.tag,
        manifest.assets.len(),
        manifest.install_sets.len()
    );
    ExitCode::SUCCESS
}
