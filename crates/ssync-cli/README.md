# ssync

A fast, memory-safe, clean-room reimplementation of `rsync` in Rust.

[![Crates.io](https://img.shields.io/crates/v/ssync.svg)](https://crates.io/crates/ssync)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Installation

```bash
cargo install ssync
```

## Features

- **Algorithmically faithful**: Full rsync delta-transfer algorithm (rolling checksum + strong digest block matching).
- **Flag compatible**: Drop-in compatible CLI flag subset matching standard `rsync`.
- **Modern & Safe**: Edition 2024, MSRV 1.85.0, strictly permissive Apache-2.0 license.
- **Wire Protocol 31**: Interoperates with upstream rsync over SSH or daemon mode.
- **Race hardened**: Held directory-file-descriptor path traversal and symlink confinement.

## Quickstart

```bash
# Local recursive sync with archive permissions and itemized changes
ssync -avi /path/to/source/ /path/to/dest/

# Remote sync over SSH
ssync -avz /path/to/source/ user@remote:/path/to/dest/

# Pull from remote
ssync -avz user@remote:/path/to/source/ /path/to/dest/

# Delta-transfer with delete extraneous files
ssync -av --delete /path/to/source/ /path/to/dest/
```

## Documentation

See the [main repository](https://github.com/ssync/ssync) for full documentation, benchmark results, and upstream test suites.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](https://github.com/ssync/ssync/blob/main/LICENSE) or http://www.apache.org/licenses/LICENSE-2.0).
