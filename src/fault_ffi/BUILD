# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0

load("@rules_cc//cc:defs.bzl", "cc_library", "cc_test")
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_shared_library", "rust_static_library", "rust_test")

FFI_SRCS = glob(["src/**/*.rs"])

FFI_DEPS = [
    "//src/common",
    "//src/fault_lib",
]

rust_library(
    name = "fault_ffi",
    srcs = FFI_SRCS,
    crate_name = "fault_ffi",
    edition = "2024",
    deps = FFI_DEPS,
)

rust_static_library(
    name = "fault_ffi_static",
    srcs = FFI_SRCS,
    crate_name = "fault_ffi",
    edition = "2024",
    visibility = ["//visibility:public"],
    deps = FFI_DEPS,
)

rust_shared_library(
    name = "fault_ffi_shared",
    srcs = FFI_SRCS,
    crate_name = "fault_ffi",
    edition = "2024",
    visibility = ["//visibility:public"],
    deps = FFI_DEPS,
)

cc_library(
    name = "c_api",
    hdrs = ["include/fault_ffi.h"],
    includes = ["include"],
    visibility = ["//visibility:public"],
    deps = [":fault_ffi_static"],
)

rust_test(
    name = "tests",
    size = "small",
    crate = ":fault_ffi",
)

cc_test(
    name = "c_consumer",
    size = "small",
    srcs = [
        "tests/abi_v1_contract.h",
        "tests/consumer.c",
    ],
    copts = [
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-pedantic",
    ],
    visibility = ["//visibility:public"],
    deps = [":c_api"],
)

rust_test(
    name = "e2e",
    size = "small",
    srcs = ["tests/e2e.rs"],
    compile_data = ["//src/fault_lib:tests/data/hvac_fault_catalog.json"],
    crate_root = "tests/e2e.rs",
    data = [
        ":c_consumer",
        ":fault_ffi_shared",
    ],
    edition = "2024",
    env = {
        "FAULT_FFI_C_CONSUMER": "$(rootpath :c_consumer)",
        "FAULT_FFI_SHARED_LIBRARY": "$(rootpath :fault_ffi_shared)",
    },
    tags = [
        "exclusive",  # The scenarios use process-global default IPC service names.
        "requires-linux",
    ],
    # Downstream language bindings can reuse the native/DFM conformance harness.
    visibility = ["//visibility:public"],
    deps = [
        ":fault_ffi",
        "//src/common",
        "//src/dfm_lib",
        "@score_fault_lib_crates//:serde_json",
        "@score_fault_lib_crates//:tempfile",
    ],
)
