use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use chiptool::{generate, ir::IR};

use crate::data::{Chip, Memory};

pub struct GeneratedChip {
    pub chip: Chip,
    pub ir: IR,
    pub feature: String,
}

pub fn write_metapac(chips: &[GeneratedChip], output_dir: &Path) -> Result<()> {
    let chips_dir = output_dir.join("src/chips");
    fs::create_dir_all(&chips_dir)
        .with_context(|| format!("creating {}", chips_dir.display()))?;

    for generated in chips {
        let chip_dir = chips_dir.join(&generated.feature);
        fs::create_dir_all(&chip_dir)?;

        let pac = strip_inner_attributes(
            &generate::render(&generated.ir, &generate::Options::new())?.to_string(),
        );
        fs::write(chip_dir.join("pac.rs"), pac)?;

        let device = generated
            .ir
            .devices
            .get("")
            .expect("generator created root device");
        fs::write(
            chip_dir.join("device.x"),
            generate::render_device_x(&generated.ir, device)?,
        )?;
        fs::write(
            chip_dir.join("memory.x"),
            render_memory_x(&generated.chip.memory),
        )?;
    }

    fs::write(output_dir.join("Cargo.toml"), render_cargo_toml(chips))?;
    fs::write(output_dir.join("build.rs"), render_build_rs(chips))?;
    fs::write(output_dir.join("src/lib.rs"), render_lib_rs())?;
    format_rust_files(output_dir)?;
    Ok(())
}

fn format_rust_files(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();

    for path in files {
        let status = Command::new("rustfmt")
            .args(["--edition", "2024", "--config", "max_width=90"])
            .arg(&path)
            .status()
            .with_context(|| format!("running rustfmt for {}", path.display()))?;
        if !status.success() {
            bail!("rustfmt failed for {}", path.display());
        }
    }
    Ok(())
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn render_memory_x(memory: &Memory) -> String {
    format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = 0x{:08X}, LENGTH = 0x{:X}\n  RAM : ORIGIN = 0x{:08X}, LENGTH = 0x{:X}\n}}\n",
        memory.flash.address, memory.flash.size, memory.ram.address, memory.ram.size
    )
}

fn render_cargo_toml(chips: &[GeneratedChip]) -> String {
    let features = chips
        .iter()
        .map(|chip| format!("{} = []", chip.feature))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"[workspace]

[package]
name = "hk32-metapac"
version = "0.1.0"
edition = "2024"
publish = false

[features]
default = []
rt = ["dep:cortex-m-rt"]
defmt = ["dep:defmt"]
{features}

[dependencies]
cortex-m = "0.7"
cortex-m-rt = {{ version = "0.7", optional = true, features = ["device"] }}
defmt = {{ version = "1", optional = true }}
"#
    )
}

fn render_build_rs(chips: &[GeneratedChip]) -> String {
    let chips = chips
        .iter()
        .map(|chip| format!("    ({:?}, {:?}),", chip.feature, chip.feature))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"use std::{{env, fs, path::PathBuf}};

const CHIPS: &[(&str, &str)] = &[
{chips}
];

fn main() {{
    let selected = CHIPS
        .iter()
        .filter(|(feature, _)| {{
            let variable = format!("CARGO_FEATURE_{{}}", feature.replace('-', "_").to_ascii_uppercase());
            env::var_os(variable).is_some()
        }})
        .collect::<Vec<_>>();

    if selected.len() != 1 {{
        panic!("enable exactly one HK32 chip feature; enabled: {{:?}}", selected);
    }}

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let chip_dir = manifest_dir.join("src/chips").join(selected[0].1);
    println!("cargo:rustc-link-search={{}}", chip_dir.display());
    println!("cargo:rerun-if-changed={{}}", chip_dir.join("pac.rs").display());
    println!("cargo:rerun-if-changed={{}}", chip_dir.join("device.x").display());
    println!("cargo:rerun-if-changed={{}}", chip_dir.join("memory.x").display());

    let selected_source = format!("include!({{:#?}});\n", chip_dir.join("pac.rs").to_string_lossy());
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("selected.rs"), selected_source).unwrap();
}}
"#
    )
}

fn render_lib_rs() -> &'static str {
    "#![no_std]\n#![allow(non_camel_case_types)]\n#![allow(non_snake_case)]\n#![allow(non_upper_case_globals)]\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/selected.rs\"));\n"
}

fn strip_inner_attributes(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut result = String::with_capacity(source.len());
    let mut copied_until = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'#' {
            index += 1;
            continue;
        }

        let mut cursor = index + 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() || bytes[cursor] != b'!' {
            index += 1;
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() || bytes[cursor] != b'[' {
            index += 1;
            continue;
        }

        let mut depth = 1;
        cursor += 1;
        while cursor < bytes.len() && depth > 0 {
            match bytes[cursor] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            break;
        }

        result.push_str(&source[copied_until..index]);
        copied_until = cursor;
        index = cursor;
    }
    result.push_str(&source[copied_until..]);
    result
}
