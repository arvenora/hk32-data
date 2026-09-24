use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Result, bail};
use chiptool::{
    ir::{self, IR},
    validate,
};

use crate::data::Chip;

pub fn build_ir(chip: &Chip, data_dir: &Path) -> Result<IR> {
    let mut ir = IR::new();
    let mut loaded_files = BTreeSet::new();

    for peripheral in &chip.peripherals {
        let Some(registers) = &peripheral.registers else {
            continue;
        };

        let path = data_dir
            .join("registers")
            .join(format!("{}.yml", registers.file));
        if loaded_files.insert(path.clone()) {
            let source: IR = crate::data::read_yaml(&path)?;
            merge_ir(
                &mut ir,
                namespace_register_ir(source, &registers.file),
                &path,
            )?;
        }

        let block = format!("{}::{}", registers.file, registers.block);
        if !ir.blocks.contains_key(&block) {
            bail!(
                "{} references block/{}, which is not defined by {}",
                peripheral.name,
                block,
                path.display()
            );
        }
    }

    let mut interrupt_names = BTreeSet::new();
    let interrupts = chip
        .interrupts
        .iter()
        .map(|interrupt| {
            if !interrupt_names.insert(&interrupt.name) {
                bail!("duplicate interrupt {}", interrupt.name);
            }
            Ok(ir::Interrupt {
                name: interrupt.name.clone(),
                description: interrupt.description.clone(),
                value: interrupt.number,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let peripherals = chip
        .peripherals
        .iter()
        .map(|peripheral| {
            let mut signals = BTreeSet::new();
            let mut interrupts = BTreeMap::new();
            for interrupt in &peripheral.interrupts {
                if !interrupt_names.contains(&interrupt.interrupt) {
                    bail!(
                        "{} signal {} references undefined interrupt {}",
                        peripheral.name,
                        interrupt.signal,
                        interrupt.interrupt
                    );
                }
                if !signals.insert(&interrupt.signal) {
                    bail!(
                        "{} defines duplicate interrupt signal {}",
                        peripheral.name,
                        interrupt.signal
                    );
                }
                interrupts.insert(interrupt.signal.clone(), interrupt.interrupt.clone());
            }
            Ok(ir::Peripheral {
                name: peripheral.name.clone(),
                description: peripheral.description.clone(),
                base_address: peripheral.address,
                array: None,
                block: peripheral
                    .registers
                    .as_ref()
                    .map(|registers| format!("{}::{}", registers.file, registers.block)),
                interrupts,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    ir.devices.insert(
        String::new(),
        ir::Device {
            nvic_priority_bits: Some(chip.core.nvic_priority_bits),
            peripherals,
            interrupts,
        },
    );
    Ok(ir)
}

fn merge_ir(target: &mut IR, source: IR, path: &Path) -> Result<()> {
    for (name, block) in source.blocks {
        if target.blocks.insert(name.clone(), block).is_some() {
            bail!("{} defines duplicate block/{}", path.display(), name);
        }
    }
    for (name, fieldset) in source.fieldsets {
        if target.fieldsets.insert(name.clone(), fieldset).is_some() {
            bail!("{} defines duplicate fieldset/{}", path.display(), name);
        }
    }
    for (name, enumm) in source.enums {
        if target.enums.insert(name.clone(), enumm).is_some() {
            bail!("{} defines duplicate enum/{}", path.display(), name);
        }
    }
    if !source.devices.is_empty() {
        bail!(
            "{} must define register blocks, not device/ entries",
            path.display()
        );
    }
    Ok(())
}

fn namespace_register_ir(mut source: IR, namespace: &str) -> IR {
    let qualify = |name: String| format!("{}::{}", namespace, name);

    for block in source.blocks.values_mut() {
        if let Some(extends) = &mut block.extends {
            *extends = qualify(extends.clone());
        }
        for item in &mut block.items {
            match &mut item.inner {
                ir::BlockItemInner::Block(block) => {
                    block.block = qualify(block.block.clone())
                }
                ir::BlockItemInner::Register(register) => {
                    if let Some(fieldset) = &mut register.fieldset {
                        *fieldset = qualify(fieldset.clone());
                    }
                }
            }
        }
    }
    for fieldset in source.fieldsets.values_mut() {
        if let Some(extends) = &mut fieldset.extends {
            *extends = qualify(extends.clone());
        }
        for field in &mut fieldset.fields {
            if let Some(enumm) = &mut field.enumm {
                *enumm = qualify(enumm.clone());
            }
        }
    }

    source.blocks = source
        .blocks
        .into_iter()
        .map(|(name, block)| (qualify(name), block))
        .collect();
    source.fieldsets = source
        .fieldsets
        .into_iter()
        .map(|(name, fieldset)| (qualify(name), fieldset))
        .collect();
    source.enums = source
        .enums
        .into_iter()
        .map(|(name, enumm)| (qualify(name), enumm))
        .collect();
    source
}

pub fn validate_ir(ir: &IR) -> Result<()> {
    let errors = validate::validate(
        ir,
        validate::Options {
            // Alternate timer register views intentionally share offsets.
            allow_register_overlap: true,
            allow_field_overlap: false,
            allow_enum_dup_value: false,
            allow_unused_enums: false,
            allow_unused_fieldsets: false,
        },
    );
    if errors.is_empty() {
        return Ok(());
    }
    bail!("chiptool IR validation failed:\n{}", errors.join("\n"));
}
