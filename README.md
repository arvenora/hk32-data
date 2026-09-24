# hk32-data

Canonical register and chip data for HK32 microcontrollers, plus the generator for
the `hk32-metapac` peripheral-access crate.

## Generate the PAC

```sh
cargo run -p hk32-metapac-gen
```

This writes a standalone crate to `generated/hk32-metapac`. The generated output is
ignored by git; `data/` and `hk32-metapac-gen/` are the maintained source.

The generated crate has one lowercase feature per chip. Enable exactly one when
using it:

```toml
hk32-metapac = { path = "generated/hk32-metapac", features = ["hk32f030mf4p6", "rt"] }
```

Pass chip names to the generator to produce a smaller development-only subset:

```sh
cargo run -p hk32-metapac-gen -- HK32F030MF4P6
```

## Data layout

- `data/chips/<chip>.yml` defines one concrete chip's memory, NVIC metadata,
  interrupt numbers, peripheral base addresses, and register-block references.
- `data/registers/<file>.yml` contains canonical chiptool IR register definitions.
  It contains no chip-specific base addresses.
- A chip peripheral reference such as `{ file: usart_v1, block: USART }` resolves
  to `data/registers/usart_v1.yml` and its `block/USART` entry.

The normal generator never reads a vendor SVD, CMSIS header, or DFP. Those files are
bootstrap and review inputs only.

## Generated repository automation

`.github/workflows/generate-metapac.yml` regenerates `arvenora/hk32-metapac` after
updates to this repository. Configure a `METAPAC_REPO_TOKEN` Actions secret with
repository contents read/write access to `arvenora/hk32-metapac`.

Generated commits and tags record the exact `hk32-data` source commit used.
