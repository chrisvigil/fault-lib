<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS) -->

# Reporter C ABI

`fault_ffi` wraps the existing Rust reporter; it does not implement another IPC
protocol. Public contracts and safety requirements are in
[`include/fault_ffi.h`](include/fault_ffi.h). The optional header-only
[`include/fault_ffi.hpp`](include/fault_ffi.hpp) provides C++17 move-only RAII handles.
See the [runnable C++ example](../../examples/cpp_reporter/README.md).

## Build and link

From the repository root:

```sh
cargo build --locked -p fault_ffi --release
```

On Linux this produces `target/release/libfault_ffi.so` and
`target/release/libfault_ffi.a`. Ship the C header with either artifact, and the
C++ header if wanted. The ABI uses the platform C calling convention and pointer
width; build for the application's target architecture. Link a **single copy** of
the reporter library into the process, rather than embedding separate static
copies in multiple plugins. Do not unload the library while handles or calls live.

Shared linking needs `-lfault_ffi` plus the library search path and a runtime
loader path. Static Linux linking also needs the native libraries reported by
`cargo rustc --locked -p fault_ffi --release -- --print native-static-libs`.

Bazel targets: `//src/fault_ffi:fault_ffi_static`,
`//src/fault_ffi:fault_ffi_shared`, `//src/fault_ffi:c_api`, and
`//src/fault_ffi:cpp_api`. The C/C++ API targets link statically.

## Contract

- Check `fl_abi_version() == FL_ABI_VERSION` before creating handles. ABI v1 has
  explicit integer constants, opaque handles, and `struct_size` on configuration.
  Existing configuration fields will not be reordered; larger tails are ignored.
- Start DFM with the same JSON catalog first. Create one context per process with
  `fl_context_create`. A failed initialization can be retried. After a successful
  initialization, further attempts return `FL_ALREADY_INITIALIZED`, **even after
  every handle is destroyed**. This follows the underlying Rust singleton.
- Create reporters using numeric, text, or raw 16-byte UUID IDs. Configuration
  supplies source attribution, lifecycle phase, and default metadata. Initialize
  configuration to zero, set `struct_size = sizeof(fl_reporter_config)`, and set
  `lifecycle_phase` explicitly (`FL_PHASE_RUNNING` for normal reporting).
- All input text is UTF-8, supplied as pointer/byte-length spans without a NUL.
  Inputs need only remain valid until the call returns. NULL/zero means an empty
  required string or an absent optional source field. NULL/nonzero is invalid.
  Non-NULL empty optional strings remain present. String capacities are in bytes,
  not Unicode characters. No input is silently truncated.
  Source fields, text fault IDs, paths, and metadata additionally require non-NUL
  ASCII because the existing iceoryx2 string container does not support Unicode.
- Publish creates the timestamp inside Rust. Each metadata list permits at most
  eight unique keys, with 64-byte keys/values. Per-call entries override matching
  defaults; the merged set must still fit eight entries. Overrides do not modify
  the reporter's defaults. Paths must obey the existing sink's path rules.
- `FL_OK` means accepted locally **or suppressed by reporter debounce**, not
  confirmed delivery or persistence. Queue saturation returns `FL_QUEUE_FULL`;
  retries/delivery failures after enqueue retain the existing worker behavior.
  Destruction is not a flush or delivery acknowledgement.
- A reporter retains its own API reference, so its context can be destroyed first.
  Destroy each non-NULL handle exactly once with its matching function; never use
  `delete`/`free`. NULL destruction is harmless. Different reporters can be used
  independently, but serialize access/move/destruction of each individual handle.
- Raw pointer validity/alignment and buffer sizes are caller obligations. Arbitrary
  dangling pointers, double destruction, and races cannot be validated by this ABI.
  Outputs must not overlap inputs or each other. Creation outputs become NULL on
  failure; destroy consumes its handle even if it returns an internal error.
- Errors are stable status codes plus a thread-local diagnostic. Retrieve it with
  `fl_last_error_message` **before cleanup**, which clears the error on success.
  That getter and `fl_abi_version` preserve the error. Buffer sizes include the
  terminating NUL; a size query returns `FL_BUFFER_TOO_SMALL`. No partial buffer
  writes occur. Rust unwinding panics are contained; aborts/OOM are not recoverable.

Enabling-condition providers/monitors, callbacks, custom timestamps, and DFM query
APIs are not part of this initial reporter ABI.

## Verify and regenerate

```sh
cargo test --locked -p fault_ffi
cargo clippy --locked -p fault_ffi --all-targets -- -D warnings
cargo +stable install cbindgen --version 0.29.2 --locked
bash src/fault_ffi/tools/generate_header.sh
bash src/fault_ffi/tools/generate_header.sh --check
bazel test //src/fault_ffi:tests //src/fault_ffi:c_consumer //src/fault_ffi:cpp_consumer //src/fault_ffi:e2e
```

Linux Cargo integration tests require `cc` and `c++` (override with `CC`/`CXX`,
executable names only). They compile both consumers and the example against the
shared library, then start their own DFM. Run without an external DFM using the
default IPC service names. Initialization scenarios use fresh subprocesses;
delivery checks poll with a deadline. Native/IPC tests are excluded under Miri;
the pure conversion and boundary unit tests remain available.

Unsafe code is confined to `src/boundary.rs` and boundary integration tests.
The original workspace's unsafe-code prohibition is unchanged for other crates.
