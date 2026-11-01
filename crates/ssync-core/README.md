# ssync-core

Core algorithms, checksums, and delta-transfer engine for [ssync](https://crates.io/crates/ssync).

[![Crates.io](https://img.shields.io/crates/v/ssync-core.svg)](https://crates.io/crates/ssync-core)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Overview

`ssync-core` provides the low-level algorithmic foundations of the rsync protocol:
- **Rolling Checksum (`Checksum1`)**: Adler32-variant rolling weak checksum with SIMD acceleration (AVX2 and unrolled SWAR) and scalar fallback.
- **Strong Digest Abstraction**: MD4, MD5, XXH64/XXH3, and SHA1 digests for signature block matching.
- **Delta Engine**: Block signature generation, hash-table lookup, sliding-window chunk matcher, and tokenized delta reconstruction.

## Usage

```toml
[dependencies]
ssync-core = "0.1.0"
```

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](https://github.com/ssync/ssync/blob/main/LICENSE) or http://www.apache.org/licenses/LICENSE-2.0).
