# ssync-fs

Filesystem traversal, metadata preservation, and path security for [ssync](https://crates.io/crates/ssync).

[![Crates.io](https://img.shields.io/crates/v/ssync-fs.svg)](https://crates.io/crates/ssync-fs)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Overview

`ssync-fs` provides safe filesystem interaction for synchronization tasks:
- **Filesystem Traversal**: Directory tree walking with relative cut points (`./`), trailing slash normalization, and empty directory pruning.
- **Metadata Management**: Nanosecond-precision mtimes, POSIX mode preservation, symlink referent extraction, and hard link cluster tracking.
- **Path Security**: Held directory-file-descriptor (`dirfd`) traversal, symlink confinement, and TOCTOU race mitigation.
- **Filter Rules**: Pattern matching engine supporting include/exclude rules, anchored patterns, CVS ignore rules, and wildmatch globs.
- **Atomic Operations**: Safe staging temporary files, backup creation, and in-place update modes.

## Usage

```toml
[dependencies]
ssync-fs = "0.2.0"
```

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](https://github.com/ssync/ssync/blob/main/LICENSE) or http://www.apache.org/licenses/LICENSE-2.0).
