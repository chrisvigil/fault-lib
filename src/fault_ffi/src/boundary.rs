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

use alloc::{boxed::Box, string::String, vec::Vec};
use core::{cell::RefCell, mem::size_of, panic::AssertUnwindSafe, ptr, slice, str};
use std::panic::catch_unwind;

use common::{
    fault::FaultId,
    ids::SourceId,
    types::{MetadataVec, ShortString},
};
use fault_lib::{
    FaultApi,
    catalog::FaultCatalogBuilder,
    reporter::{Reporter, ReporterApi, ReporterConfig},
};

use crate::{
    model::{Error, merge_metadata, phase, short, stage},
    types::{
        FL_ABI_VERSION, FL_BUFFER_TOO_SMALL, FL_CATALOG_ERROR, FL_INTERNAL_ERROR,
        FL_METADATA_CAPACITY, FL_OK, FL_PATH_CAPACITY, FlMetadata, FlReporterConfig, FlStatus,
        FlString,
    },
};

/// Opaque owner of the process-wide reporter API. Initialization succeeds once
/// per process; destroying this handle does not permit reinitialization.
pub struct FlContext {
    api: FaultApi,
    catalog_id: String,
}

/// Opaque reporter. Callers must serialize access to each individual handle.
/// Retains the API even if its context handle is destroyed first.
pub struct FlReporter {
    reporter: Reporter,
    _api: FaultApi,
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_last_error(message: Option<String>) {
    // Foreign thread-local destructors may call us after Rust TLS has gone away.
    // A panic hook can also reenter while the diagnostic is borrowed.
    let _ = LAST_ERROR.try_with(|slot| {
        if let Ok(mut value) = slot.try_borrow_mut() {
            if let Some(message) = message {
                *value = message;
            } else {
                value.clear();
            }
        }
    });
}

fn guard(operation: impl FnOnce() -> Result<(), Error>) -> FlStatus {
    let result = catch_unwind(AssertUnwindSafe(operation));
    let error = match result {
        Ok(Ok(())) => {
            set_last_error(None);
            return FL_OK;
        }
        Ok(Err(error)) => error,
        Err(payload) => {
            // A panic payload can have a panicking destructor. Never drop it across C.
            if let Err(nested) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                core::mem::forget(nested);
            }
            Error::new(
                FL_INTERNAL_ERROR,
                "Rust panic contained at the C ABI boundary",
            )
        }
    };
    set_last_error(Some(error.message));
    error.status
}

unsafe fn text<'a>(value: FlString) -> Result<&'a str, Error> {
    if value.len == 0 {
        return Ok("");
    }
    if value.data.is_null() || isize::try_from(value.len).is_err() {
        return Err(Error::invalid("invalid string pointer/length"));
    }
    // SAFETY: the exported function's caller guarantees a readable span for this call.
    let bytes = unsafe { slice::from_raw_parts(value.data, value.len) };
    str::from_utf8(bytes).map_err(|_| Error::invalid("string is not UTF-8"))
}

unsafe fn optional_short(value: FlString) -> Result<Option<common::types::ShortString>, Error> {
    // SAFETY: forwarded from the caller's input-span contract.
    let value_text = unsafe { text(value)? };
    if value.data.is_null() {
        Ok(None)
    } else {
        short(value_text).map(Some)
    }
}

unsafe fn metadata(data: *const FlMetadata, len: usize) -> Result<MetadataVec, Error> {
    if len > FL_METADATA_CAPACITY {
        return Err(Error::invalid("metadata exceeds eight entries"));
    }
    if len == 0 {
        return Ok(MetadataVec::new());
    }
    if data.is_null() {
        return Err(Error::invalid("NULL metadata with nonzero length"));
    }
    // SAFETY: the caller guarantees len initialized and aligned metadata entries.
    let entries = unsafe { slice::from_raw_parts(data, len) };
    let mut result: Vec<(ShortString, ShortString)> = Vec::new();
    for entry in entries {
        // SAFETY: each entry's strings obey the same input-span contract.
        let key = short(unsafe { text(entry.key)? })?;
        let value = short(unsafe { text(entry.value)? })?;
        if result.iter().any(|(existing, _)| existing == &key) {
            return Err(Error::invalid("duplicate metadata key"));
        }
        result.push((key, value));
    }
    MetadataVec::try_from(result.as_slice())
        .map_err(|_| Error::invalid("metadata exceeds eight entries"))
}

unsafe fn config(value: *const FlReporterConfig) -> Result<ReporterConfig, Error> {
    if value.is_null() {
        return Err(Error::invalid("NULL reporter configuration"));
    }
    // SAFETY: caller guarantees at least the initialized, aligned struct_size field.
    if unsafe { ptr::addr_of!((*value).struct_size).read() } < size_of::<FlReporterConfig>() {
        return Err(Error::invalid("reporter configuration is too small"));
    }
    // SAFETY: struct_size and the caller's contract guarantee the complete v1 prefix.
    let value = unsafe { &*value };
    // SAFETY: configuration string spans and metadata obey the caller's contract.
    unsafe {
        Ok(ReporterConfig {
            source: SourceId {
                entity: short(text(value.source_entity)?)?,
                ecu: optional_short(value.source_ecu)?,
                domain: optional_short(value.source_domain)?,
                sw_component: optional_short(value.source_sw_component)?,
                instance: optional_short(value.source_instance)?,
            },
            lifecycle_phase: phase(value.lifecycle_phase)?,
            default_env_data: metadata(value.default_metadata, value.default_metadata_len)?,
        })
    }
}

unsafe fn output<T>(out: *mut *mut T) -> Result<(), Error> {
    if out.is_null() {
        return Err(Error::invalid("NULL output handle"));
    }
    // SAFETY: caller guarantees writable, aligned output storage, not aliasing inputs.
    unsafe { out.write(ptr::null_mut()) };
    Ok(())
}

unsafe fn create_reporter(
    context: *const FlContext,
    id: impl FnOnce() -> Result<FaultId, Error>,
    options: *const FlReporterConfig,
    out: *mut *mut FlReporter,
) -> FlStatus {
    guard(|| {
        // SAFETY: forwarded output storage contract.
        unsafe { output(out)? };
        // SAFETY: caller guarantees a live context or NULL.
        let context = unsafe { context.as_ref() }.ok_or_else(|| Error::invalid("NULL context"))?;
        let id = id()?;
        // SAFETY: forwarded configuration contract.
        let options = unsafe { config(options)? };
        let reporter = Reporter::new(&id, options)?;
        let handle = Box::new(FlReporter {
            reporter,
            _api: context.api.clone(),
        });
        // SAFETY: output validated above, ownership transfers to caller.
        unsafe { out.write(Box::into_raw(handle)) };
        Ok(())
    })
}

unsafe fn copy_string(
    value: &str,
    buffer: *mut u8,
    capacity: usize,
    required: *mut usize,
) -> Result<(), Error> {
    if required.is_null() {
        return Err(Error::invalid("NULL required-size output"));
    }
    let needed = value
        .len()
        .checked_add(1)
        .ok_or_else(|| Error::invalid("string size overflow"))?;
    // SAFETY: caller guarantees writable output, not aliasing the buffer or inputs.
    unsafe { required.write(needed) };
    if capacity < needed {
        return Err(Error::new(
            FL_BUFFER_TOO_SMALL,
            "output buffer is too small",
        ));
    }
    if buffer.is_null() {
        return Err(Error::invalid("NULL output buffer with nonzero capacity"));
    }
    // SAFETY: caller guarantees a writable non-overlapping buffer of capacity bytes.
    unsafe {
        ptr::copy_nonoverlapping(value.as_ptr(), buffer, value.len());
        buffer.add(value.len()).write(0);
    }
    Ok(())
}

/// Return the supported ABI major version. Does not change the last error.
#[unsafe(no_mangle)]
pub extern "C" fn fl_abi_version() -> u32 {
    FL_ABI_VERSION
}

/// Initialize once per process using catalog JSON. DFM must already be running
/// with the same catalog. On failure, `*out_context` is NULL. Catalog/transport
/// failures can be retried; successful initialization cannot be reset by destroying
/// handles. A contained panic does not guarantee rollback of process-wide state.
/// A shared library must remain loaded until process exit, including after failure.
///
/// # Safety
/// Input spans must be readable for the call. `out_context` must be writable,
/// aligned, and must not alias inputs. Non-NULL pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_context_create(
    catalog_json: FlString,
    out_context: *mut *mut FlContext,
) -> FlStatus {
    guard(|| {
        // SAFETY: forwarded input/output contracts.
        unsafe { output(out_context)? };
        let json = unsafe { text(catalog_json)? };
        let catalog = FaultCatalogBuilder::new()
            .json_string(json)
            .and_then(FaultCatalogBuilder::try_build)
            .map_err(|e| Error::new(FL_CATALOG_ERROR, e.to_string()))?;
        catalog
            .try_id()
            .map_err(|e| Error::new(FL_CATALOG_ERROR, e.to_string()))?;
        let catalog_id = catalog.id.to_string();
        let api = FaultApi::try_new(catalog)?;
        let handle = Box::new(FlContext { api, catalog_id });
        // SAFETY: output validated above; caller owns the resulting handle.
        unsafe { out_context.write(Box::into_raw(handle)) };
        Ok(())
    })
}

/// Copy the catalog ID, including a terminating NUL. required includes that NUL.
/// A NULL buffer with capacity=0 queries the size and returns `FL_BUFFER_TOO_SMALL`.
///
/// # Safety
/// context must be live or NULL. required must be writable. buffer must be writable
/// for capacity bytes when non-NULL. Outputs must not alias each other or inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_context_catalog_id(
    context: *const FlContext,
    buffer: *mut u8,
    capacity: usize,
    required: *mut usize,
) -> FlStatus {
    guard(|| {
        // SAFETY: caller guarantees a live context or NULL.
        let context = unsafe { context.as_ref() }.ok_or_else(|| Error::invalid("NULL context"))?;
        // SAFETY: forwarded output buffer contract.
        unsafe { copy_string(&context.catalog_id, buffer, capacity, required) }
    })
}

/// Create a reporter for a numeric ID. On failure, `*out_reporter` is NULL.
///
/// # Safety
/// context must be live or NULL. config must contain its readable initialized v1
/// prefix (or just `struct_size` when too small). All nested spans must be valid.
/// `out_reporter` must be writable, aligned, and must not alias inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_reporter_create_numeric(
    context: *const FlContext,
    fault_id: u32,
    config: *const FlReporterConfig,
    out_reporter: *mut *mut FlReporter,
) -> FlStatus {
    // SAFETY: forwarded handle, configuration, and output contracts.
    unsafe {
        create_reporter(
            context,
            || Ok(FaultId::Numeric(fault_id)),
            config,
            out_reporter,
        )
    }
}

/// Create a reporter for a non-NUL ASCII text ID (maximum 64 bytes).
/// On failure, `*out_reporter` is NULL.
///
/// # Safety
/// Same contract as `fl_reporter_create_numeric`; `fault_id` must also be readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_reporter_create_text(
    context: *const FlContext,
    fault_id: FlString,
    config: *const FlReporterConfig,
    out_reporter: *mut *mut FlReporter,
) -> FlStatus {
    // SAFETY: forwarded handle, input span, configuration, and output contracts.
    unsafe {
        create_reporter(
            context,
            || Ok(FaultId::Text(short(text(fault_id)?)?)),
            config,
            out_reporter,
        )
    }
}

/// Create a reporter for a UUID represented as exactly 16 raw bytes, not UUID text.
/// On failure, `*out_reporter` is NULL.
///
/// # Safety
/// Same contract as `fl_reporter_create_numeric`. uuid must point to 16 readable bytes
/// or be NULL (which returns `FL_INVALID_ARGUMENT`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_reporter_create_uuid(
    context: *const FlContext,
    uuid: *const u8,
    config: *const FlReporterConfig,
    out_reporter: *mut *mut FlReporter,
) -> FlStatus {
    // SAFETY: forwarded handle, UUID span, configuration, and output contracts.
    unsafe {
        create_reporter(
            context,
            || {
                if uuid.is_null() {
                    return Err(Error::invalid("NULL UUID"));
                }
                let mut id = [0; 16];
                ptr::copy_nonoverlapping(uuid, id.as_mut_ptr(), id.len());
                Ok(FaultId::Uuid(id))
            },
            config,
            out_reporter,
        )
    }
}

/// Create a timestamped record and publish through the existing reporter.
/// Per-call metadata overrides matching default keys; the merged set must have
/// at most eight entries. Duplicate keys within either input list are rejected.
/// `FL_OK` means accepted locally OR suppressed by debounce, not delivered/persisted.
///
/// # Safety
/// reporter must be live and exclusively accessed for this call, or NULL.
/// All string/metadata spans must be readable. No operation, including destruction,
/// may race with use of the same reporter. Inputs must not alias the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_reporter_publish(
    reporter: *mut FlReporter,
    entity_path: FlString,
    lifecycle_stage: u32,
    entries: *const FlMetadata,
    entries_len: usize,
) -> FlStatus {
    guard(|| {
        // SAFETY: caller guarantees exclusive access to a live reporter or NULL.
        let handle = unsafe { reporter.as_mut() }.ok_or_else(|| Error::invalid("NULL reporter"))?;
        let stage = stage(lifecycle_stage)?;
        // SAFETY: forwarded input-span contracts.
        let path = unsafe { text(entity_path)? };
        if path.len() > FL_PATH_CAPACITY || path.is_empty() {
            return Err(Error::invalid("entity path must contain 1..128 bytes"));
        }
        let overrides = unsafe { metadata(entries, entries_len)? };
        let mut record = handle.reporter.create_record(stage);
        record.env_data = merge_metadata(&record.env_data, &overrides)?;
        handle.reporter.publish(path, record)?;
        Ok(())
    })
}

/// Destroy a context; NULL is harmless. Live reporters keep the API alive.
/// This does not reset process-wide initialization or guarantee delivery of queued events.
/// The handle is consumed even if an internal error is returned.
/// Destruction does not establish worker quiescence or permit shared-library unloading.
///
/// # Safety
/// context must be NULL or a live handle returned by `fl_context_create`, not already
/// destroyed. Destruction must not race with any operation using this handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_context_destroy(context: *mut FlContext) -> FlStatus {
    guard(|| {
        if !context.is_null() {
            // SAFETY: caller transfers its uniquely owned handle back to Rust.
            drop(unsafe { Box::from_raw(context) });
        }
        Ok(())
    })
}

/// Destroy a reporter; NULL is harmless. Does not guarantee delivery of queued events.
/// The handle is consumed even if an internal error is returned.
/// Destruction does not establish worker quiescence or permit shared-library unloading.
///
/// # Safety
/// reporter must be NULL or a live handle returned by a reporter creation function,
/// not already destroyed. Destruction must not race with any use of this handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_reporter_destroy(reporter: *mut FlReporter) -> FlStatus {
    guard(|| {
        if !reporter.is_null() {
            // SAFETY: caller transfers its uniquely owned handle back to Rust.
            drop(unsafe { Box::from_raw(reporter) });
        }
        Ok(())
    })
}

/// Copy this thread's last error as NUL-terminated UTF-8. required includes the NUL.
/// NULL buffer/capacity=0 queries the size (`FL_BUFFER_TOO_SMALL`). No partial writes.
/// Does not modify the last error. Successful operations clear the error, except
/// `fl_abi_version` and this function. Capture errors before calling destroy functions.
///
/// # Safety
/// required must be writable. buffer must be writable for capacity bytes when
/// non-NULL. Output spans must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fl_last_error_message(
    buffer: *mut u8,
    capacity: usize,
    required: *mut usize,
) -> FlStatus {
    LAST_ERROR
        .try_with(|slot| {
            let Ok(error) = slot.try_borrow() else {
                return FL_INTERNAL_ERROR;
            };
            // SAFETY: forwarded output-buffer contract. No guard, to retain error.
            match unsafe { copy_string(&error, buffer, capacity, required) } {
                Ok(()) => FL_OK,
                Err(error) => error.status,
            }
        })
        .unwrap_or(FL_INTERNAL_ERROR)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn invalid_spans_and_metadata_are_rejected() {
        // SAFETY: all non-NULL pointers below refer to live stack data for each call.
        unsafe {
            assert!(
                text(FlString {
                    data: ptr::null(),
                    len: 1
                })
                .is_err()
            );
            assert!(
                text(FlString {
                    data: [0xff].as_ptr(),
                    len: 1
                })
                .is_err()
            );
            assert!(text(FlString::default()).unwrap().is_empty());
            assert!(metadata(ptr::null(), 1).is_err());
            assert!(metadata(ptr::null(), 9).is_err());
            let pair = FlMetadata {
                key: "a".into(),
                value: "b".into(),
            };
            assert!(metadata([pair, pair].as_ptr(), 2).is_err());
            assert!(config(ptr::null()).is_err());
            let small: usize = 0;
            assert!(config(ptr::from_ref(&small).cast()).is_err());
        }
    }

    #[test]
    fn panic_is_contained_and_error_copy_is_stable() {
        assert_eq!(guard(|| panic!("test panic")), FL_INTERNAL_ERROR);
        let mut required = 0;
        // SAFETY: size output and allocated buffer remain valid and don't overlap.
        unsafe {
            assert_eq!(
                fl_last_error_message(ptr::null_mut(), 0, &raw mut required),
                FL_BUFFER_TOO_SMALL
            );
            let mut buffer = alloc::vec![0; required];
            assert_eq!(
                fl_last_error_message(buffer.as_mut_ptr(), buffer.len(), &raw mut required),
                FL_OK
            );
            assert!(str::from_utf8(&buffer).unwrap().contains("panic contained"));
            assert_eq!(fl_abi_version(), FL_ABI_VERSION);
            assert_eq!(
                fl_last_error_message(ptr::null_mut(), 0, &raw mut required),
                FL_BUFFER_TOO_SMALL
            );
            assert_eq!(fl_reporter_destroy(ptr::null_mut()), FL_OK);
            assert_eq!(required, buffer.len());
            assert_eq!(
                fl_last_error_message(ptr::null_mut(), 0, &raw mut required),
                FL_BUFFER_TOO_SMALL
            );
            assert_eq!(required, 1);
        }
    }

    #[test]
    fn panicking_payload_destructor_is_contained() {
        struct PanickingDrop;
        impl Drop for PanickingDrop {
            fn drop(&mut self) {
                // The nested payload would also panic if the guard dropped it.
                std::panic::panic_any(Self);
            }
        }

        assert_eq!(
            guard(|| std::panic::panic_any(PanickingDrop)),
            FL_INTERNAL_ERROR
        );
        LAST_ERROR.with(|slot| assert!(slot.borrow().contains("panic contained")));
    }

    #[test]
    fn opaque_owners_support_cross_thread_transfer() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FlContext>();
        assert_send_sync::<FlReporter>();
    }

    #[test]
    fn configuration_and_metadata_capacities() {
        let keys: Vec<_> = (0..8).map(|i| i.to_string()).collect();
        let value = "v".repeat(64);
        let pairs: Vec<_> = keys
            .iter()
            .map(|key| FlMetadata {
                key: key.as_str().into(),
                value: value.as_str().into(),
            })
            .collect();
        let options = FlReporterConfig {
            // A future extension is accepted as long as the v1 prefix is readable.
            struct_size: size_of::<FlReporterConfig>().saturating_add(8),
            source_entity: value.as_str().into(),
            source_ecu: "".into(),
            source_domain: "domain".into(),
            source_sw_component: "component".into(),
            source_instance: "instance".into(),
            default_metadata: pairs.as_ptr(),
            default_metadata_len: pairs.len(),
            ..FlReporterConfig::default()
        };
        // SAFETY: options and every nested span refer to live allocated buffers.
        unsafe {
            let converted = config(&raw const options).unwrap();
            assert_eq!(converted.source.entity, short(&value).unwrap());
            assert_eq!(converted.source.ecu, Some(short("").unwrap()));
            assert_eq!(converted.source.domain, Some(short("domain").unwrap()));
            assert_eq!(
                converted.source.sw_component,
                Some(short("component").unwrap())
            );
            assert_eq!(converted.source.instance, Some(short("instance").unwrap()));
            assert_eq!(converted.default_env_data.iter().count(), 8);
            let too_long = "v".repeat(65);
            let invalid = FlReporterConfig {
                source_entity: too_long.as_str().into(),
                ..options
            };
            assert!(config(&raw const invalid).is_err());
            let invalid = FlMetadata {
                key: too_long.as_str().into(),
                value: "v".into(),
            };
            assert!(metadata(&raw const invalid, 1).is_err());
            let invalid = FlMetadata {
                key: "key".into(),
                value: too_long.as_str().into(),
            };
            assert!(metadata(&raw const invalid, 1).is_err());
        }
    }

    #[test]
    fn null_outputs_and_diagnostic_reentrancy() {
        // SAFETY: NULL inputs are explicitly permitted as invalid-argument probes;
        // every non-NULL output points to writable, non-overlapping live storage.
        unsafe {
            assert!(output::<FlContext>(ptr::null_mut()).is_err());
            assert!(copy_string("text", ptr::null_mut(), 0, ptr::null_mut()).is_err());
            let mut required = 0;
            assert!(copy_string("text", ptr::null_mut(), 5, &raw mut required).is_err());
            assert_eq!(required, 5);
            let mut buffer = [42; 4];
            assert!(
                copy_string("text", buffer.as_mut_ptr(), buffer.len(), &raw mut required).is_err()
            );
            assert_eq!(buffer, [42; 4]);
            LAST_ERROR.with(|slot| {
                let _borrow = slot.borrow_mut();
                assert_eq!(
                    fl_last_error_message(ptr::null_mut(), 0, &raw mut required),
                    FL_INTERNAL_ERROR
                );
                assert_eq!(guard(|| Ok(())), FL_OK);
                assert_eq!(
                    guard(|| Err(Error::invalid("reentrant"))),
                    crate::FL_INVALID_ARGUMENT
                );
            });
        }
    }

    #[test]
    fn errors_are_thread_local() {
        set_last_error(Some("parent".into()));
        std::thread::spawn(|| {
            set_last_error(Some("child".into()));
            LAST_ERROR.with(|slot| assert_eq!(slot.borrow().as_str(), "child"));
        })
        .join()
        .unwrap();
        LAST_ERROR.with(|slot| assert_eq!(slot.borrow().as_str(), "parent"));
    }
}
