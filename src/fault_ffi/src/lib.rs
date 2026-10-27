/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 */

//! C ABI for the reporter-side fault library. See `include/fault_ffi.h` and README.md.
#![deny(unsafe_code, unsafe_op_in_unsafe_fn)]

extern crate alloc;

#[allow(unsafe_code)]
mod boundary;
mod model;
mod types;

pub use boundary::*;
pub use types::*;
