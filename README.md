# mdat-rs (microscopy data, Rust)

Rust workspace for **mdat**: a native CLI and WebAssembly bindings for reading
ND2 (Nikon) and CZI (Zeiss) microscopy files. **mdat** stands for
**microscopy data**.

## Workspace

| Crate | Description |
| --- | --- |
| `crates/mdat` | Rust CLI for ND2/CZI microscopy data utilities. |
| `crates/mdat-wasm` | WebAssembly bindings for ND2/CZI microscopy readers. |

## Build

Build the CLI:

```bash
cargo build --release
# Binary at target/release/mdat
```

Build with SMB source support (reads ND2/CZI files over SMB):

```bash
cargo build --release --features smb
```

Build the WASM package (requires [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)):

```bash
./scripts/build-wasm.sh
# Output at crates/mdat-wasm/pkg/
```

## CLI usage

```bash
mdat --help
mdat convert --help
mdat metadata --help
```

### `convert`

Export image data to TIFF files under an output directory.

```bash
# Full conversion (mdat layout, default)
mdat convert sample.nd2 --output out -y
mdat convert sample.czi --output out -y

# Cell-ACDC layout
mdat convert sample.nd2 --output out --format acdc -y

# Subset: positions 0–4 and 10, timepoints 0–49 and 100, channels 0 and 2, z 0–9
mdat convert sample.nd2 --output out \
  --position 0:5,10 \
  --time 0:50,100 \
  --channel 0,2 \
  --z 0:10 \
  -y
```

| Option | Description |
| --- | --- |
| `-o`, `--output` | Output directory (required). |
| `--format` | `mdat` (default) or `acdc` (Cell-ACDC). |
| `--position` | Positions to export. Default: `all`. |
| `--time` | Timepoints to export. Default: `all`. |
| `--channel` | Channels to export. Default: `all`. |
| `--z` | Z-slices to export. Default: `all`. |
| `-y`, `--yes` | Skip the confirmation prompt. |

Before writing files, `convert` prints a summary of the input dimensions and the
selected positions, timepoints, and channels. Without `-y`, it asks for
confirmation.

**Selection syntax** (`--position`, `--time`, `--channel`, `--z`):

- `all` — every index along that axis.
- Comma-separated indices — e.g. `0,2,4`.
- Python-style slices — e.g. `0:10` (start:end), `0:10:2` (start:end:step).
- Mix slices and indices — e.g. `0:5,10`.

Indices are **0-based** and refer to the original axis order in the source file.
For timepoints, exported filenames use a **renumbered** index (`t_new`: 0, 1, …)
while `time_map.csv` (mdat layout) records the mapping back to the original
indices.

#### Output layout: `mdat` (default)

One folder per position, one TIFF per `(channel, time, z)` frame:

```
out/
  Pos0/
    time_map.csv
    img_channel000_position000_time000000000_z000.tif
    img_channel000_position000_time000000001_z000.tif
    ...
  Pos1/
    ...
```

`time_map.csv` columns: `t` (exported index), `t_real` (original timepoint index).

#### Output layout: `acdc`

Cell-ACDC-compatible layout: one folder per position, one **stacked** TIFF per
channel (T×Z or Z-only), plus a metadata CSV:

```
out/
  Position_1/
    Images/
      sample_s01_metadata.csv
      sample_s01_GFP.tif
      sample_s01_phase_contrast.tif
  Position_2/
    ...
```

Position folders are **1-based** (`Position_1`, `Position_2`, …). Channel TIFF
names come from normalized channel metadata when available.

### `metadata`

Inspect file metadata without converting image data.

```bash
# Normalized JSON to stdout
mdat metadata sample.czi

# Normalized JSON to file
mdat metadata sample.czi --output sample.metadata.json

# Raw metadata payload (OME-XML for ND2, vendor XML for CZI)
mdat metadata sample.nd2 --raw --output sample.metadata.xml
mdat metadata sample.czi --raw --output sample.metadata.xml
```

| Option | Description |
| --- | --- |
| `-o`, `--output` | Write to this file instead of stdout. |
| `--raw` | Export the native metadata payload instead of normalized JSON. |

## WASM bindings

The `mdat-wasm` crate exposes the ND2/CZI readers to JavaScript via
[`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/). After running
`./scripts/build-wasm.sh`, import the generated package from
`crates/mdat-wasm/pkg/`.