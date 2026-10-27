# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0

load("@rules_cc//cc:defs.bzl", "cc_binary")

cc_binary(
    name = "cpp_reporter",
    srcs = ["main.cpp"],
    copts = [
        "-std=c++17",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-pedantic",
    ],
    visibility = ["//visibility:public"],
    deps = ["//src/fault_ffi:cpp_api"],
)
