use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chiptool::{ir::IR, validate};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chip {
    pub name: String,
    pub memory: Memory,
    pub core: Core,
    #[serde(default)]
    pub interrupts: Vec<Interrupt>,
    pub peripherals: Vec<Peripheral>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub flash: Region,
    pub ram: Region,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub address: u64,
    pub size: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Core {
    pub arch: String,
    pub nvic_priority_bits: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interrupt {
    pub name: String,
    pub number: u32,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peripheral {
    pub name: String,
    pub address: u64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub registers: Option<RegisterBlock>,
    #[serde(default)]
    pub interrupts: Vec<PeripheralInterrupt>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterBlock {
    /// Basename of data/registers/<file>.yml.
    pub file: String,
    /// Name of the block/ entry in that file.
    pub block: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralInterrupt {
    pub signal: String,
    pub interrupt: String,
}

pub fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading canonical data file {}", path.display()))?;
    serde_yaml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))
}

pub fn chip_paths(data_dir: &Path, selected: &[String]) -> Result<Vec<PathBuf>> {
    let chips_dir = data_dir.join("chips");
    if !selected.is_empty() {
        return selected
            .iter()
            .map(|chip| {
                let path = chips_dir.join(format!("{chip}.yml"));
                if !path.is_file() {
                    bail!("unknown chip {chip:?}: {} does not exist", path.display());
                }
                Ok(path)
            })
            .collect();
    }

    let mut paths = fs::read_dir(&chips_dir)
        .with_context(|| format!("reading {}", chips_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "yml"))
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        bail!("no chip YAML files found in {}", chips_dir.display());
    }
    Ok(paths)
}

pub fn validate_chip_name(chip: &Chip, path: &Path) -> Result<()> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("chip path has no UTF-8 file stem")?;
    if chip.name != stem {
        bail!(
            "chip file {} declares {:?}, expected {:?}",
            path.display(),
            chip.name,
            stem
        );
    }
    if chip.core.arch != "cortex-m0" {
        bail!("{}: unsupported core {:?}", chip.name, chip.core.arch);
    }
    Ok(())
}

pub fn feature_name(chip_name: &str) -> Result<String> {
    if !chip_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("chip name {chip_name:?} cannot be a Cargo feature");
    }
    Ok(chip_name.to_ascii_lowercase())
}

pub fn validate_register_files(data_dir: &Path) -> Result<()> {
    let registers_dir = data_dir.join("registers");
    let mut paths = fs::read_dir(&registers_dir)
        .with_context(|| format!("reading {}", registers_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "yml"))
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let ir: IR = read_yaml(&path)?;
        let errors = validate::validate(
            &ir,
            validate::Options {
                // Alternate timer register views intentionally share offsets.
                allow_register_overlap: true,
                allow_field_overlap: false,
                allow_enum_dup_value: false,
                allow_unused_enums: false,
                allow_unused_fieldsets: false,
            },
        );
        if !errors.is_empty() {
            bail!(
                "chiptool IR validation failed for {}:\n{}",
                path.display(),
                errors.join("\n")
            );
        }
    }
    Ok(())
}
