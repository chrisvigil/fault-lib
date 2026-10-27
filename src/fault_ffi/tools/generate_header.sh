#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
if [[ "$(cbindgen --version)" != "cbindgen 0.29.2" ]]; then
    echo 'Install the pinned generator: cargo +stable install cbindgen --version 0.29.2 --locked' >&2
    exit 1
fi
args=()
if [[ "${1:-}" == "--check" ]]; then
    args+=(--verify)
elif [[ $# -ne 0 ]]; then
    echo "Usage: $0 [--check]" >&2
    exit 1
fi
cbindgen "$root/src/fault_ffi" --config "$root/src/fault_ffi/cbindgen.toml" \
    --output "$root/src/fault_ffi/include/fault_ffi.h" "${args[@]}"
