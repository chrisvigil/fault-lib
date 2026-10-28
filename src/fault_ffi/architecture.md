<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS) -->

# C ABI: S-Core and OpenSOVD architecture assessment

Reviewed 2026-10-28 against the local ABI-v1 implementation and the sources below.
**Retain the provider-owned C ABI, downstream C++17 wrapper, and static integration
default.** The interface design fits the cited interoperability guidance. Full
S-Core safety qualification and opted-in shared-delivery compliance are not
established by the current implementation.

## Authority and applicability

| Source | Status / scope | Consequence for this API |
| --- | --- | --- |
| [DR-002-Arch, parent decision][delivery] | **Accepted**, 2026-05-15; its section title still says “Decision Proposal” | Static by default; a module may additionally opt into shared delivery with justification and design-review approval. Its mandatory shared-delivery controls are conditional on that opt-in. |
| [DR-002-Arch, cross-language considerations][considerations] | Supporting rationale and practical recommendations | `extern "C"` wrappers and cbindgen are appropriate for both static and dynamic linking. Preference for shared runtime encapsulation is an argument for an opt-in, not a replacement for the accepted default. |
| [ABI-compatible data type requirements][abi-req] | Valid, ASIL-B-tagged **component requirements** for baselibs ABI-compatible containers | Apply to that component's shared-memory data representations; they are not a blanket prohibition on pointers or allocation at every C function boundary. |
| [ABI-types component overview][abi-scope] | Defines zero-copy IPC between C++17/Rust processes, including differing word sizes | Pointer-free, fixed-width, preallocated containers are necessary when interpreting the same transferred memory in different processes. Our borrowed spans/configs are copied into Rust-owned types before IPC. |
| [OpenSOVD design][opensovd-design] | Initial high-level design; detailed interfaces still to be specified | Fault Library provides app-facing reporting, configuration and IPC to DFM. Client libraries need safe-component quality and “FFI guarantees”; this phrase appears in a safety/non-interference discussion and is not expanded there. Panic containment alone cannot satisfy that safety statement. |
| [OpenSOVD/S-Core interface ADR][opensovd-adr] | **Accepted**, 2025-07-21 | Fault Library is the technical/organizational interface, co-developed by both projects. S-Core is responsible for additional safety work up to ASIL-B; OpenSOVD remains independently usable. It does not dictate the Git repository for a C bridge. |
| [OpenSOVD issue #108][integration] and its 14 comments | Open; explicitly a starting point, not final until reviewed | Gateway/CDA build and integration proposals do not prescribe this reporter ABI. The [latest comment][reporter-question] asks about fault-lib/iceoryx2 versus mw::com and adapter ownership; no answer was present at review. |

The source's ASIL tags are requirements/classification, not evidence that these
bindings satisfy them. Likewise, C++17 alignment is not a MISRA compliance result.

## Representation, ownership and errors

The implemented boundary is `application -> C ABI -> Rust reporter -> iceoryx2 ->
DFM`. C++ convenience types never enter the binary interface.

- `#[repr(C)]` structs, `extern "C"` functions and explicit `uint32_t`/`int32_t`
  values avoid Rust enum niches, Rust `String`/`Vec` layouts and C++ library ABI.
  Stage/phase inputs are integers validated before conversion, so unknown values
  produce statuses rather than invalid Rust enum values.
- `fl_string` is a borrowed byte pointer plus `size_t`, not a Rust slice. The
  caller/library must share the target C ABI, including word size and alignment.
  Same-process pointers and `size_t` are appropriate here. Requirements
  `comp_req__abi_compatible_data_types__int_fix` and `...__nop_mt` concern transferred
  IPC types; applying them here would unnecessarily change ABI v1. The underlying
  `common` IPC payloads need their own assessment if a new transport requires them
  to implement baselibs ABI-container contracts.
- Struct field order is stable. `struct_size` rejects undersized configuration
  and permits unknown trailing bytes. Future extensions must still accept the
  original v1 prefix; simply increasing the implementation's minimum `sizeof`
  would break old callers. No packed layout or host-independent wire format is
  promised. The C++ header uses standard-layout C structs, verified by its test.
- Strings/metadata are validated and copied during calls. Counts and capacities
  are checked; UTF-8 is checked, and the existing IPC string types impose additional
  non-NUL ASCII limits on IDs/source/path/metadata. Catalog JSON remains UTF-8.
  There is no promise of arbitrary Unicode support in these IPC fields.
- Opaque handles intentionally hide Rust layouts. Rust allocates and frees its
  own `Box` objects through paired create/destroy functions. Callers must never
  `free`/`delete` them. A reporter owns a `FaultApi` clone, preserving catalog/sink
  lifetime after context destruction. C++ move-only RAII preserves this ownership.
- Statuses plus caller-owned output buffers and thread-local diagnostics are an
  appropriate C error model. No Rust `Result`, C++ exception, allocator-owned string
  or mandatory `AbiResult` container crosses the interface. Failed C factories null
  their output; failed C++ factories preserve their destination. Destruction consumes
  a handle even on an internal error. Capture diagnostics before cleanup.
- Pointer provenance, alignment, initialized buffer extent, non-overlap and exclusive
  use are caller obligations. Null/length/enum checks do not validate forged pointers.
  An opaque-pointer API cannot make use-after-free or concurrent destruction safe.

## Panic, thread and lifetime guarantees

`src/boundary.rs::guard` catches unwinding panics, returns `FL_INTERNAL_ERROR`, and
also catches panics from dropping the caught payload. A nested payload is forgotten
to prevent its destructor unwinding across C. The trivial version getter and
diagnostic getter preserve TLS state; the latter uses fallible TLS access/borrowing
rather than panicking on teardown/reentrancy. They do not execute reporter work.

This is sufficient **unwind containment for valid ABI calls using `panic=unwind`**,
not transactional recovery. Abort-mode builds, OOM aborts, an aborting panic hook,
double panics during stack unwinding, memory corruption and worker-thread failures
are outside it. A contained panic can leave mutable state partially changed; continued
reporting is not guaranteed. No exception callbacks are exposed in this ABI.

Initialization uses the existing mutex/`OnceLock`; only one success is allowed per
process, even after all owners die. Normal failed initialization can be retried.
Callers serialize operations/moves/destruction on a particular handle. Independent
reporters may run on different threads. Rust tests assert the actual opaque owners
are `Send + Sync`; C++ tests exercise exclusive transfer and independent reporters,
and foreign-thread diagnostics. These guarantees do not make the library lock-free,
async-signal-safe, allocation-free or bounded-latency.

### Corrected unloading contract

**Keep a shared library loaded until process exit after any use.** Merely destroying
all handles and waiting for calls to return is insufficient:

1. `fault_manager_sink.rs::Drop` waits up to two seconds via a watchdog thread;
   timeout abandons the join, and a self-drop skips it. Even a successful worker
   result does not join the watchdog itself before returning.
2. `FaultManagerSink::with_ec_manager` can return early after worker creation; its
   local `JoinHandle` then detaches. Failed initialization is not proof of quiescence.
3. Calling threads may retain Rust TLS destructors whose code belongs to the DSO.

The generated header and both READMEs now state the process-lifetime requirement.
No unload-safe shutdown/flush operation currently exists. Adding one would require
an underlying runtime/TLS lifecycle design, not just another wrapper function.
`FL_OK` remains local acceptance or debounce suppression, not delivery/persistence;
destruction remains best-effort, not a delivery acknowledgement.

## Static versus shared delivery

Use the existing Bazel `c_api` static target for source-built S-Core integration.
It fits DR-002's default, Bzlmod's one-way cross-repository dependency graph and the
current consumer/example builds. Link **one reporter runtime per process**; separate
static copies in plugins would duplicate singleton state. Static here describes the
reporter library, not a claim that the complete application has no shared OS libraries.

Keep the shared targets for development/integration and a future justified opt-in.
Cross-language runtime encapsulation, multiple processes sharing code or independent
updates could justify one. Prefer DR-002 Mode A (build from source in integration)
first. Mode B (independently released prebuilt libraries) additionally needs validated
old-consumer/new-provider combinations and release support policy. Unloading/hot reload
is unsupported in either mode; update on process restart.

### Conditional shared-delivery gaps

These are requirements for an opted-in S-Core release, not reasons to break the
current static C API. Inspected native Linux artifacts: Cargo release and Bazel
`bazel-bin/src/fault_ffi/libfault_ffi.so` (fastbuild).

| DR-002 requirement | Current evidence / gap | Needed for an opted-in release |
| --- | --- | --- |
| Controlled C exports and hidden internals | Both inspected artifacts have exactly ten externally visible `fl_*` definitions. Rust cdylib export control is effective; `#[doc(hidden)]` only affects documentation. Bazel's local TLS symbol-table entries are not external exports. | Preserve the export gate on every supported release toolchain/target; apply language-appropriate visibility settings. |
| Explicit initialization | `fl_context_create` performs catalog/IPC setup explicitly, after DFM starts; no reporter constructor hook | Preserve explicit order. The example `score_<module>_init` spelling is not a reason to rename an existing API. |
| SONAME and semantic versioning | Neither artifact has a SONAME; workspace package is 0.0.1 while the C ABI major is 1 | Define the release/ABI-major policy and versioned `.so` installation/symlinks. `fl_abi_version` is not a loader-level SONAME substitute. |
| Eager binding and full RELRO | Both have BIND_NOW and GNU_RELRO in the inspected builds | Enforce and check these properties in release toolchains, including consumers' call sites; defaults observed locally are not a release guarantee. |
| Hardening | Both are shared ELF objects with non-executable GNU_STACK and build IDs. Full stack-protector/fortify/toolchain coverage is unverified. | Qualify applicable Rust and native dependency hardening. C/C++ flags cannot simply be passed to rustc; `_FORTIFY_SOURCE` does not harden Rust bounds operations. |
| RPATH, not RUNPATH, and controlled paths | DSOs have neither tag; the inspected native C++ test executable has RUNPATH to the Cargo build directory | Define installed platform-controlled paths and disable new dtags in the release integration. Development test paths are not a deployment policy. |
| Static internal dependencies | Inspected DT_NEEDED lists only libc, loader and libgcc_s; no separate fault_lib/common/iceoryx2 DSOs | Retain this composition and review the complete release dependency set. |
| ABI-compliance CI | Header freshness was previously the only explicit declaration check. Added frozen v1 layout/type/signature/value checks and actual shared-export checks in existing Cargo/Bazel CI. | Preserve those checks; add supported-release binary baselines/`abidiff` or a qualified equivalent and semantic compatibility tests for independent updates. The local baseline is not full release qualification. |
| Toolchain identity CI gate | Cargo pins nightly-2025-07-14, Bazel pins Rust 1.91.0; downstream Bazel selects 1.91.0 too. No identity gate covers native compilers and all consumers. | Agree the mixed-language interpretation of the decision's same-compiler/version requirement and enforce one approved toolchain set per deployment. Pinning two build paths is not such a gate. |
| Deployment manifest and debug archives | Build IDs exist; no release manifest of versions/checksums/build IDs or corresponding debug archive policy | Implement with the selected release packaging/integration owner. |
| Documented/reviewed opt-in | No approval established by the cited issue or these local changes | Record deployment need and obtain the module design review required by DR-002 before claiming opted-in delivery compliance. |

## Repository responsibility

Keep Rust ABI implementation, generated C header, unsafe adaptation, error mapping,
versioning and C conformance tests in **fault-lib**. They evolve with `Reporter`,
`FaultApi`, catalogs and the IPC containers. Keep header-only C++17 RAII, C++ tests,
and examples in **cpp-bindings**, whose declared purpose is C++ application bindings.
The dependency remains `cpp-bindings -> fault-lib`, with no provider build dependency
on C++. Neither moving the bridge nor using `cxx` would improve the guarantees above.

S-Core-specific adapters, deployment decisions and safety evidence belong with the
agreed S-Core integration responsibilities. Issue #108 has not resolved the concrete
`DTC` adapter location or future transport. Preserve the existing iceoryx2 reporting
path rather than inventing HTTP or mw::com requirements from that discussion.

The paired-checkout override still needs replacement by an immutable provider
revision after publication. Cross-repository conformance runs already exercise actual
C/C++ reports through DFM. Formal freedom-from-interference, timing/resource analysis,
ASIL qualification and MISRA assessment remain integration/safety work; the cited
documents do not supply that evidence for this implementation.

[delivery]: https://eclipse-score.github.io/score/v0.7.0/design_decisions/DR-002-arch.html
[considerations]: https://eclipse-score.github.io/score/v0.7.0/design_decisions/DR-002-arch/considerations.html#cross-language-interoperability-c-rust
[abi-req]: https://eclipse-score.github.io/reference_integration/main/modules/score_baselibs/baselibs/components/abi_compatible_data_types/docs/requirements/index.html
[abi-scope]: https://eclipse-score.github.io/reference_integration/main/modules/score_baselibs/baselibs/components/abi_compatible_data_types/docs/index.html
[opensovd-design]: https://github.com/eclipse-opensovd/opensovd/blob/main/docs/design/design.md
[opensovd-adr]: https://github.com/eclipse-opensovd/opensovd/blob/main/docs/design/adr/001-adr-score-interface.md
[integration]: https://github.com/eclipse-opensovd/opensovd/issues/108
[reporter-question]: https://github.com/eclipse-opensovd/opensovd/issues/108#issuecomment-6417085298
