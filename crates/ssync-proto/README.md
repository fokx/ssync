# ssync-proto

rsync wire protocol 31 implementation and multiplexing for [ssync](https://crates.io/crates/ssync).

[![Crates.io](https://img.shields.io/crates/v/ssync-proto.svg)](https://crates.io/crates/ssync-proto)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Overview

`ssync-proto` implements the wire serialization, framing, and negotiation protocols used by `rsync`:
- **Protocol 31 Framing**: Handshake negotiation and compatibility flag bitmasks (`compat_flags`).
- **Varint / Varlong Encoding**: Compact variable-length integer wire codecs.
- **Multiplexing (`mplex`)**: Channelized transport over standard I/O streams (`MPLEX_BASE`, `MSG_DATA`, `MSG_INFO`, `MSG_ERROR_*`).
- **File List Serialization (`flist`)**: Path prefix compression, attribute packing, and wire transmission.
- **Transfer Token Streaming**: Block signature and delta token serialization over multiplexed streams.

## Usage

```toml
[dependencies]
ssync-proto = "0.1.0"
```

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](https://github.com/ssync/ssync/blob/main/LICENSE) or http://www.apache.org/licenses/LICENSE-2.0).
