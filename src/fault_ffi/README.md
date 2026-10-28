<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS) -->

# Reporter C ABI

`fault_ffi` wraps the existing Rust reporter; it does not implement another IPC
protocol. Public contracts and safety requirements are in
[`include/fault_ffi.h`](include/fault_ffi.h). The optional C++17 move-only RAII
wrapper and its runnable example are maintained in
[`cpp-bindings`](https://github.com/eclipse-opensovd/cpp-bindings).
The C ABI is independently usable from C; this repository does not depend on
`cpp-bindings` for builds or tests.

The [S-Core/OpenSOVD architecture assessment](architecture.md) records which
requirements apply, the linking recommendation, and remaining deployment gaps.

## Build and link

From the repository root:

```sh
cargo build --locked -p fault_ffi --release
```

On Linux this produces `target/release/libfault_ffi.so` and
`target/release/libfault_ffi.a`. Ship the generated C header with either artifact.
The ABI uses the platform C calling convention and pointer
width; build for the application's target architecture. Link a **single copy** of
the reporter library into the process, rather than embedding separate static
copies in multiple plugins. **After any use, keep a shared library loaded until
process exit**, including after failed initialization or destruction of all handles.
Worker teardown is best-effort, and thread-local destructors may still need its code.

Shared linking needs `-lfault_ffi` plus the library search path and a runtime
loader path. Static Linux linking also needs the native libraries reported by
`cargo rustc --locked -p fault_ffi --release -- --print native-static-libs`.

Bazel targets: `//src/fault_ffi:fault_ffi_static`,
`//src/fault_ffi:fault_ffi_shared`, and `//src/fault_ffi:c_api`.
The C API target links statically. The former `:cpp_api` target is now
`@opensovd_cpp_bindings//:fault_reporter`; existing C++ source includes and
`fault::` APIs are preserved, but consumers must use that dependency/include path.
Static linking is the recommended S-Core integration default. The shared targets
support development/integration testing; a qualified S-Core shared-library release
also needs the versioning, packaging, and CI controls listed in the assessment.

## Contract

- Check `fl_abi_version() == FL_ABI_VERSION` before creating handles. ABI v1 has
  explicit integer constants, opaque handles, and `struct_size` on configuration.
  Existing configuration fields will not be reordered; larger tails are ignored.
  Spans/configuration are same-process call arguments, not shared-memory payloads;
  `size_t` and pointers intentionally follow the target C ABI.
- Start DFM with the same JSON catalog first. Create one context per process with
  `fl_context_create`. Catalog/transport initialization failures can be retried;
  contained panics do not guarantee rollback. After a successful initialization,
  further attempts return `FL_ALREADY_INITIALIZED`, **even after
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
  writes occur. With `panic=unwind`, boundary panics return `FL_INTERNAL_ERROR`;
  this does not promise rollback, continued reporter service, or worker recovery.
  Aborts (including OOM, `panic=abort`, and double panics during unwinding) cannot
  become status errors. A panic hook runs before containment and must not abort.
- Calls are not async-signal-safe or guaranteed allocation-free/bounded-latency.
  Input storage must not be concurrently modified. Per-handle serialization and
  panic containment are programming contracts, not freedom-from-interference or
  ASIL qualification evidence on their own.

Enabling-condition providers/monitors, callbacks, custom timestamps, and DFM query
APIs are not part of this initial reporter ABI.

## Verify and regenerate

```sh
cargo test --locked -p fault_ffi
cargo clippy --locked -p fault_ffi --all-targets -- -D warnings
cargo +stable install cbindgen --version 0.29.2 --locked
bash src/fault_ffi/tools/generate_header.sh
bash src/fault_ffi/tools/generate_header.sh --check
bazel test //src/fault_ffi:tests //src/fault_ffi:c_consumer //src/fault_ffi:e2e
```

Linux integration tests require GNU `nm` (binutils); Cargo additionally requires
`cc` (override with `CC`, executable name only). The suite checks the actual
shared library's externally visible exports against the ten-symbol v1 baseline.
`tests/abi_v1_contract.h` freezes C field types/offsets, sizes/alignment, constants,
and all function signatures independently of header regeneration. It is compiled
as part of the C consumer in both Cargo and Bazel CI; baseline changes require ABI
review. These checks and the live C calls cover the current ABI, not arbitrary
historical binary compatibility or semantic compatibility across releases.

Cargo tests compile the C consumer against the shared library, then start their
own DFM and verify C reporting as well as the Rust boundary. Run without an
external DFM using the default IPC service names. Initialization scenarios use
fresh subprocesses; delivery checks poll with a deadline. Native/IPC tests are excluded under Miri;
the pure conversion and boundary unit tests remain available.

Unsafe code is confined to `src/boundary.rs` and boundary integration tests.
The original workspace's unsafe-code prohibition is unchanged for other crates.

### Downstream conformance testing

`cpp-bindings/tools/test_cargo.sh` builds its C++17 executables against this
crate's shared library and runs this same DFM harness. Bazel consumers can use
the public test executable `//src/fault_ffi:e2e` with the filter
`--exact native_consumers_and_ipc_scenarios`. It accepts these environment paths:

- `FAULT_FFI_C_CONSUMER`: prebuilt C consumer (Bazel); Cargo builds it if unset.
- `FAULT_FFI_SHARED_LIBRARY`: shared artifact to check (Bazel); defaults to the
  Cargo test executable's adjacent `libfault_ffi.so`. Supply it when invoking a
  Bazel-built harness directly, since Bazel's test environment is not inherited.
- `FAULT_FFI_CPP_CONSUMER` and `FAULT_FFI_EXAMPLE`: optional downstream binaries;
  supply both or neither. Missing/unusable paths fail the suite.

The downstream consumer must accept a catalog filename, publish the fixture's
text fault with `application=cpp_consumer`, and exit successfully. With no args
it must run its standalone error/ownership checks. The example accepts
`<catalog> <text-id> failed|passed`, publishes `application=cpp_reporter` and
`temperature=42` on failure, and reports recovery on `passed`. The harness
queries DFM for these effects, including recovery; process exit alone is not
considered successful delivery. These are test fixture conventions, not public
application ABI requirements. The JSON fixture is embedded at compile time so
the harness also runs as an external Bazel dependency.

The C-only suite remains the default. All IPC suites must run sequentially on
a host using the default service names; Bazel marks this target `exclusive`.
