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

use core::{mem::size_of, ptr};

/// ABI major version. Incompatible contracts require a new major version.
pub const FL_ABI_VERSION: u32 = 1;

pub type FlStatus = i32;
pub const FL_OK: FlStatus = 0;
pub const FL_INVALID_ARGUMENT: FlStatus = 1;
pub const FL_ALREADY_INITIALIZED: FlStatus = 2;
pub const FL_UNKNOWN_FAULT: FlStatus = 3;
pub const FL_CATALOG_ERROR: FlStatus = 4;
pub const FL_TRANSPORT_ERROR: FlStatus = 5;
pub const FL_TIMEOUT: FlStatus = 6;
pub const FL_QUEUE_FULL: FlStatus = 7;
pub const FL_PERMISSION_DENIED: FlStatus = 8;
pub const FL_RATE_LIMITED: FlStatus = 9;
pub const FL_INTERNAL_ERROR: FlStatus = 10;
pub const FL_BUFFER_TOO_SMALL: FlStatus = 11;
pub const FL_NOT_INITIALIZED: FlStatus = 12;

pub const FL_STAGE_NOT_TESTED: u32 = 0;
pub const FL_STAGE_PREFAILED: u32 = 1;
pub const FL_STAGE_FAILED: u32 = 2;
pub const FL_STAGE_PREPASSED: u32 = 3;
pub const FL_STAGE_PASSED: u32 = 4;

pub const FL_PHASE_INIT: u32 = 0;
pub const FL_PHASE_RUNNING: u32 = 1;
pub const FL_PHASE_SUSPEND: u32 = 2;
pub const FL_PHASE_RESUME: u32 = 3;
pub const FL_PHASE_SHUTDOWN: u32 = 4;

pub const FL_SHORT_STRING_CAPACITY: usize = 64;
pub const FL_PATH_CAPACITY: usize = 128;
pub const FL_METADATA_CAPACITY: usize = 8;

/// Borrowed UTF-8 bytes, without a terminating NUL. NULL is allowed only with len=0.
/// Inputs are read/copied during the call and never retained as pointers.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FlString {
    pub data: *const u8,
    pub len: usize,
}

impl Default for FlString {
    fn default() -> Self {
        Self {
            data: ptr::null(),
            len: 0,
        }
    }
}

impl From<&str> for FlString {
    fn from(value: &str) -> Self {
        Self {
            data: value.as_ptr(),
            len: value.len(),
        }
    }
}

/// A metadata key/value pair. Both strings have a maximum of 64 bytes and must
/// contain non-NUL ASCII, as required by the existing IPC string container.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FlMetadata {
    pub key: FlString,
    pub value: FlString,
}

/// Initialize to zero, then set `struct_size=sizeof(fl_reporter_config)`, `source_entity`,
/// and `lifecycle_phase`. NULL optional source strings mean absent; non-NULL empty
/// strings mean present but empty. Unknown trailing fields are ignored.
/// Source fields are non-NUL ASCII strings of at most 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FlReporterConfig {
    pub struct_size: usize,
    pub source_entity: FlString,
    pub source_ecu: FlString,
    pub source_domain: FlString,
    pub source_sw_component: FlString,
    pub source_instance: FlString,
    pub lifecycle_phase: u32,
    pub default_metadata: *const FlMetadata,
    pub default_metadata_len: usize,
}

impl Default for FlReporterConfig {
    fn default() -> Self {
        Self {
            struct_size: size_of::<Self>(),
            source_entity: FlString::default(),
            source_ecu: FlString::default(),
            source_domain: FlString::default(),
            source_sw_component: FlString::default(),
            source_instance: FlString::default(),
            lifecycle_phase: FL_PHASE_RUNNING,
            default_metadata: ptr::null(),
            default_metadata_len: 0,
        }
    }
}
