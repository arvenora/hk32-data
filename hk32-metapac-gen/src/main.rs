mod data;
mod ir;
mod output;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

use crate::data::{
    chip_paths, feature_name, read_yaml, validate_chip_name, validate_register_files,
};
use crate::ir::{build_ir, validate_ir};
use crate::output::{GeneratedChip, write_metapac};

#[derive(Parser)]
#[command(about = "Generate an HK32 metapac from canonical YAML data")]
struct Args {
    /// Chip names to generate. With no names, generates every data/chips/*.yml file.
    #[arg(value_name = "CHIP")]
    chips: Vec<String>,

    /// Directory containing chips/ and registers/.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    /// Generated metapac crate directory.
    #[arg(long, default_value = "generated/hk32-metapac")]
    output_dir: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    validate_register_files(&args.data_dir)?;

    let paths = chip_paths(&args.data_dir, &args.chips)?;
    let mut generated = Vec::with_capacity(paths.len());
    let mut features = std::collections::BTreeSet::new();

    for path in paths {
        let chip = read_yaml(&path)?;
        validate_chip_name(&chip, &path)?;
        let feature = feature_name(&chip.name)?;
        if !features.insert(feature.clone()) {
            bail!("multiple chips resolve to Cargo feature {feature}");
        }

        let ir = build_ir(&chip, &args.data_dir)?;
        validate_ir(&ir)?;
        generated.push(GeneratedChip { chip, ir, feature });
    }

    write_metapac(&generated, &args.output_dir)?;
    println!(
        "generated {} chip(s) in {}",
        generated.len(),
        args.output_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_data_is_loadable() {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");
        validate_register_files(&data_dir).unwrap();

        for path in chip_paths(&data_dir, &[]).unwrap() {
            let chip = read_yaml(&path).unwrap();
            validate_chip_name(&chip, &path).unwrap();
            validate_ir(&build_ir(&chip, &data_dir).unwrap()).unwrap();
        }
    }
}
