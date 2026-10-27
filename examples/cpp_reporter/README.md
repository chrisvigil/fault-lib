<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS) -->

# C++17 fault reporter

Run all commands from the repository root. Prerequisites: the pinned Rust
toolchain, a C++17 compiler, and the existing iceoryx2 runtime requirements.

Build the shared Rust library and the C++ application:

```sh
cargo build --locked -p fault_ffi --release
c++ -o target/cpp_reporter -std=c++17 -Wall -Wextra -Werror -pedantic \
    examples/cpp_reporter/main.cpp -Isrc/fault_ffi/include \
    -Ltarget/release -lfault_ffi -Wl,-rpath,"$PWD/target/release"
```

Start DFM in one terminal (choose a persistent storage directory):

```sh
cargo run --locked -p dfm_bin -- \
    --catalog-dir src/fault_lib/tests/data --storage-dir target/cpp-dfm-storage
```

After `DFM ready`, run in another terminal:

```sh
target/cpp_reporter src/fault_lib/tests/data/hvac_fault_catalog.json
```

The application reads the catalog, creates a reporter, releases its context,
then reports `Failed -> PrePassed -> Passed` with source identity and metadata.
It uses `hvac.blower.speed_sensor_mismatch`, which has no reporter debounce. The
numeric HVAC example has a 60-second hold time and is unsuitable for a quick demo.
Demo recovery events are spaced by 200ms for this fault's 100ms manager cooldown.

To leave the fault failed, or report recovery in another process:

```sh
target/cpp_reporter src/fault_lib/tests/data/hvac_fault_catalog.json hvac.blower.speed_sensor_mismatch failed
target/cpp_reporter src/fault_lib/tests/data/hvac_fault_catalog.json hvac.blower.speed_sensor_mismatch passed
```

With Bazel, build/run the statically linked equivalent:

```sh
bazel build //examples/cpp_reporter
bazel run //examples/cpp_reporter -- "$PWD/src/fault_lib/tests/data/hvac_fault_catalog.json"
```

Successful publication is asynchronous local acceptance, not confirmation from
DFM; reporter debounce can also suppress records. The program does not claim
to flush on exit. Automated ABI tests query DFM to verify eventual state.
See [ABI contracts and integration guidance](../../src/fault_ffi/README.md).
