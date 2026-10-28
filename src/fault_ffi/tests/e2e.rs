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

//! Linux native-link and IPC tests. Each initialization scenario runs in a fresh
//! process because `FaultApi` deliberately cannot be reset after successful init.
#![cfg(all(target_os = "linux", not(miri)))]
#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc
)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    ptr,
    time::{Duration, Instant},
};

use common::{
    catalog::{FaultCatalogBuilder, FaultCatalogConfig},
    fault::FaultId,
};
use dfm_lib::{
    DfmQueryApi, Iceoryx2DfmQuery, diagnostic_fault_manager::DiagnosticFaultManager,
    fault_catalog_registry::FaultCatalogRegistry, sovd_fault_storage::KvsSovdFaultStateStorage,
};
use fault_ffi::*;

fn success(output: Output) {
    let Output {
        status,
        stdout,
        stderr,
    } = output;
    assert!(
        status.success(),
        "process failed: {}\nstdout: {}\nstderr: {}",
        status,
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
}

fn source_root() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
}

fn shared_library() -> PathBuf {
    env::var_os("FAULT_FFI_SHARED_LIBRARY").map_or_else(
        || {
            env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("libfault_ffi.so")
        },
        PathBuf::from,
    )
}

fn check_shared_exports() {
    // Frozen v1 export set. Catch internal symbols escaping as well as missing API.
    // -g excludes local TLS symbols present in some linkers' dynamic symbol tables.
    let output = Command::new("nm")
        .args(["-D", "-g", "--defined-only", "--format=posix"])
        .arg(shared_library())
        .output()
        .expect("GNU nm (binutils) is required for the ABI tests");
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    success(output);
    let mut symbols: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    symbols.sort_unstable();
    assert_eq!(
        symbols,
        [
            "fl_abi_version",
            "fl_context_catalog_id",
            "fl_context_create",
            "fl_context_destroy",
            "fl_last_error_message",
            "fl_reporter_create_numeric",
            "fl_reporter_create_text",
            "fl_reporter_create_uuid",
            "fl_reporter_destroy",
            "fl_reporter_publish",
        ],
        "shared-library ABI exports changed; review compatibility"
    );
}

fn compile(source: &Path, output: &Path) {
    let compiler = env::var("CC").unwrap_or_else(|_| "cc".into());
    let libraries = env::current_exe().unwrap().parent().unwrap().to_path_buf();
    assert!(
        libraries.join("libfault_ffi.so").exists(),
        "Cargo did not produce the shared library"
    );
    success(
        Command::new(compiler)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic"])
            .arg(source)
            .arg("-I")
            .arg(source_root().join("include"))
            .arg("-L")
            .arg(&libraries)
            .arg(format!("-Wl,-rpath,{}", libraries.display()))
            .args(["-lfault_ffi", "-o"])
            .arg(output)
            .output()
            .expect("a native C compiler is required for the ABI tests"),
    );
}

#[test]
fn native_consumers_and_ipc_scenarios() {
    check_shared_exports();
    let dir = tempfile::tempdir().unwrap();
    // Bazel supplies a compiled C consumer. Cargo compiles the same source
    // against its cdylib. This repository needs no C++ wrapper or compiler.
    let c = if let Ok(c) = env::var("FAULT_FFI_C_CONSUMER") {
        PathBuf::from(c)
    } else {
        let c = dir.path().join("consumer_c");
        compile(&source_root().join("tests/consumer.c"), &c);
        c
    };
    let c = fs::canonicalize(c).unwrap();
    success(Command::new(&c).output().unwrap());
    // Optional downstream executables implement the fixture protocol documented
    // in README.md. Only paths cross the repository boundary, never source deps.
    let cpp = env::var("FAULT_FFI_CPP_CONSUMER").ok();
    let example = env::var("FAULT_FFI_EXAMPLE").ok();
    assert_eq!(
        cpp.is_some(),
        example.is_some(),
        "supply both downstream executables"
    );
    let downstream = cpp.zip(example).map(|(cpp, example)| {
        (
            fs::canonicalize(cpp).unwrap(),
            fs::canonicalize(example).unwrap(),
        )
    });
    if let Some((cpp, _)) = &downstream {
        success(Command::new(cpp).output().unwrap());
    }
    for scenario in ["offline", "mismatch", "positive"] {
        let mut child = Command::new(env::current_exe().unwrap());
        child
            .args(["--exact", "scenario_child", "--nocapture"])
            .env("FAULT_FFI_SCENARIO", scenario)
            .env("FAULT_FFI_C_CONSUMER", &c);
        if let Some((cpp, example)) = &downstream {
            child
                .env("FAULT_FFI_CPP_CONSUMER", cpp)
                .env("FAULT_FFI_EXAMPLE", example);
        }
        success(child.output().unwrap());
    }
}

fn fixture() -> FaultCatalogConfig {
    let mut config: FaultCatalogConfig = serde_json::from_str(include_str!(
        "../../fault_lib/tests/data/hvac_fault_catalog.json"
    ))
    .unwrap();
    // Keep tests fast; the repository's numeric HVAC example has a 60s debounce.
    for fault in &mut config.faults {
        fault.reporter_side_debounce = None;
        fault.manager_side_debounce = None;
    }
    let mut debounced = config.faults.first().unwrap().clone();
    debounced.id = FaultId::Numeric(0x7002);
    debounced.reporter_side_debounce = Some(common::debounce::DebounceMode::CountWithinWindow {
        min_count: 2,
        window: Duration::from_secs(60).into(),
    });
    config.faults.push(debounced);
    let mut uuid = config.faults.first().unwrap().clone();
    uuid.id = FaultId::Uuid([7; 16]);
    config.faults.push(uuid);
    config
}

fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now().checked_add(Duration::from_secs(5)).unwrap();
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "DFM did not reach the expected state"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[allow(clippy::too_many_lines)] // One stateful scenario; initialization cannot be reset.
fn scenario_child() {
    let Ok(scenario) = env::var("FAULT_FFI_SCENARIO") else {
        return;
    };
    let fixture = fixture();
    let json = serde_json::to_string(&fixture).unwrap();
    let mut context = ptr::null_mut();
    if scenario == "offline" {
        // SAFETY: JSON and writable output storage remain live for the call.
        assert_eq!(
            unsafe { fl_context_create(json.as_str().into(), &raw mut context) },
            FL_TIMEOUT
        );
        assert!(context.is_null());
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let storage = KvsSovdFaultStateStorage::new(dir.path(), 0).unwrap();
    let catalog = FaultCatalogBuilder::new()
        .cfg_struct(fixture.clone())
        .unwrap()
        .try_build()
        .unwrap();
    let dfm = DiagnosticFaultManager::with_query_server(
        storage,
        FaultCatalogRegistry::new(vec![catalog]),
    );
    wait_for(|| Iceoryx2DfmQuery::new().is_ok());
    if scenario == "mismatch" {
        let mut wrong = fixture;
        wrong.version = wrong.version.saturating_add(1);
        let wrong = serde_json::to_string(&wrong).unwrap();
        // SAFETY: JSON and writable output storage remain live for the call.
        assert_eq!(
            unsafe { fl_context_create(wrong.as_str().into(), &raw mut context) },
            FL_CATALOG_ERROR
        );
        assert!(context.is_null());
        // A failed handshake must not permanently consume initialization.
    }

    // SAFETY: all ABI calls below use owned live handles and live input buffers;
    // no handle is used after destruction, and no operations race.
    unsafe {
        assert_eq!(
            fl_context_create(json.as_str().into(), &raw mut context),
            FL_OK
        );
        let mut duplicate = ptr::null_mut();
        assert_eq!(
            fl_context_create(json.as_str().into(), &raw mut duplicate),
            FL_ALREADY_INITIALIZED
        );
        assert!(duplicate.is_null());
        let mut required = 0;
        assert_eq!(
            fl_context_catalog_id(context, ptr::null_mut(), 0, &raw mut required),
            FL_BUFFER_TOO_SMALL
        );
        let mut id_buffer = vec![0; required];
        assert_eq!(
            fl_context_catalog_id(
                context,
                id_buffer.as_mut_ptr(),
                id_buffer.len(),
                &raw mut required
            ),
            FL_OK
        );
        assert_eq!(id_buffer, b"hvac\0");

        let default = FlMetadata {
            key: "origin".into(),
            value: "default".into(),
        };
        let config = FlReporterConfig {
            source_entity: "ffi_test".into(),
            source_ecu: "ECU-A".into(),
            default_metadata: &raw const default,
            default_metadata_len: 1,
            ..FlReporterConfig::default()
        };
        let mut reporter = ptr::null_mut();
        assert_eq!(
            fl_reporter_create_numeric(context, u32::MAX, &raw const config, &raw mut reporter),
            FL_UNKNOWN_FAULT
        );
        assert!(reporter.is_null());
        let small: usize = 0;
        assert_eq!(
            fl_reporter_create_numeric(
                context,
                0x7001,
                ptr::from_ref(&small).cast(),
                &raw mut reporter
            ),
            FL_INVALID_ARGUMENT
        );
        let invalid = FlReporterConfig {
            lifecycle_phase: u32::MAX,
            ..config
        };
        assert_eq!(
            fl_reporter_create_numeric(context, 0x7001, &raw const invalid, &raw mut reporter),
            FL_INVALID_ARGUMENT
        );
        assert_eq!(
            fl_reporter_create_numeric(context, 0x7001, &raw const config, &raw mut reporter),
            FL_OK
        );
        let mut debounced = ptr::null_mut();
        assert_eq!(
            fl_reporter_create_numeric(context, 0x7002, &raw const config, &raw mut debounced),
            FL_OK
        );
        for path in ["../bad", "hvac\0bad", "hväc"] {
            assert_eq!(
                fl_reporter_publish(debounced, path.into(), FL_STAGE_FAILED, ptr::null(), 0),
                FL_INVALID_ARGUMENT
            );
        }
        assert_eq!(fl_reporter_destroy(debounced), FL_OK);
        let mut uuid_reporter = ptr::null_mut();
        assert_eq!(
            fl_reporter_create_uuid(
                context,
                ptr::null(),
                &raw const config,
                &raw mut uuid_reporter
            ),
            FL_INVALID_ARGUMENT
        );
        assert_eq!(
            fl_reporter_create_uuid(
                context,
                [7; 16].as_ptr(),
                &raw const config,
                &raw mut uuid_reporter
            ),
            FL_OK
        );
        let mut text_reporter = ptr::null_mut();
        let long_id = "x".repeat(65);
        assert_eq!(
            fl_reporter_create_text(
                context,
                long_id.as_str().into(),
                &raw const config,
                &raw mut text_reporter
            ),
            FL_INVALID_ARGUMENT
        );
        assert_eq!(
            fl_reporter_create_text(
                context,
                "hvac.blower.speed_sensor_mismatch".into(),
                &raw const config,
                &raw mut text_reporter
            ),
            FL_OK
        );
        assert_eq!(fl_context_destroy(context), FL_OK);
        assert_eq!(
            fl_reporter_publish(
                uuid_reporter,
                "hvac".into(),
                FL_STAGE_FAILED,
                ptr::null(),
                0
            ),
            FL_OK
        );
        let query = dfm.query_api();
        wait_for(|| {
            query
                .get_fault("hvac", "07070707-0707-0707-0707-070707070707")
                .is_ok_and(|(fault, _)| {
                    fault
                        .typed_status
                        .is_some_and(|s| s.test_failed == Some(true))
                })
        });
        assert_eq!(
            fl_reporter_publish(reporter, "hvac".into(), 999, ptr::null(), 0),
            FL_INVALID_ARGUMENT
        );
        assert_eq!(
            fl_reporter_publish(reporter, "../bad".into(), FL_STAGE_FAILED, ptr::null(), 0),
            FL_INVALID_ARGUMENT
        );
        let long_path = "x".repeat(129);
        assert_eq!(
            fl_reporter_publish(
                reporter,
                long_path.as_str().into(),
                FL_STAGE_FAILED,
                ptr::null(),
                0
            ),
            FL_INVALID_ARGUMENT
        );
        assert_eq!(
            fl_reporter_publish(reporter, "hvac".into(), FL_STAGE_FAILED, ptr::null(), 9),
            FL_INVALID_ARGUMENT
        );
        let overrides = FlMetadata {
            key: "origin".into(),
            value: "override".into(),
        };
        assert_eq!(
            fl_reporter_publish(
                reporter,
                "hvac".into(),
                FL_STAGE_FAILED,
                &raw const overrides,
                1
            ),
            FL_OK
        );
        wait_for(|| {
            query.get_fault("hvac", "0x7001").is_ok_and(|(fault, env)| {
                fault
                    .typed_status
                    .is_some_and(|s| s.test_failed == Some(true))
                    && env.get("origin").is_some_and(|v| v == "override")
            })
        });
        assert_eq!(
            fl_reporter_publish(reporter, "hvac".into(), FL_STAGE_PREPASSED, ptr::null(), 0),
            FL_OK
        );
        assert_eq!(
            fl_reporter_publish(reporter, "hvac".into(), FL_STAGE_PASSED, ptr::null(), 0),
            FL_OK
        );
        wait_for(|| {
            query.get_fault("hvac", "0x7001").is_ok_and(|(fault, _)| {
                fault
                    .typed_status
                    .is_some_and(|s| s.test_failed == Some(false) && s.confirmed_dtc == Some(false))
            })
        });
        assert_eq!(fl_reporter_destroy(reporter), FL_OK);
        assert_eq!(fl_reporter_destroy(uuid_reporter), FL_OK);
        assert_eq!(fl_reporter_destroy(text_reporter), FL_OK);
        assert_eq!(
            fl_context_create(json.as_str().into(), &raw mut duplicate),
            FL_ALREADY_INITIALIZED
        );
    }

    if scenario != "positive" {
        return;
    }
    let catalog_path = dir.path().join("catalog.json");
    fs::write(&catalog_path, json).unwrap();
    success(
        Command::new(env::var("FAULT_FFI_C_CONSUMER").unwrap())
            .arg(&catalog_path)
            .output()
            .unwrap(),
    );
    let query = dfm.query_api();
    wait_for(|| {
        query
            .get_fault("hvac", "hvac.blower.speed_sensor_mismatch")
            .is_ok_and(|(_, env)| env.get("application").is_some_and(|v| v == "c_consumer"))
    });
    let Ok(cpp) = env::var("FAULT_FFI_CPP_CONSUMER") else {
        return;
    };
    success(Command::new(cpp).arg(&catalog_path).output().unwrap());
    let query = dfm.query_api();
    wait_for(|| {
        query
            .get_fault("hvac", "hvac.blower.speed_sensor_mismatch")
            .is_ok_and(|(_, env)| env.get("application").is_some_and(|v| v == "cpp_consumer"))
    });
    let example = env::var("FAULT_FFI_EXAMPLE").unwrap();
    success(
        Command::new(&example)
            .arg(&catalog_path)
            .args(["hvac.blower.speed_sensor_mismatch", "failed"])
            .output()
            .unwrap(),
    );
    wait_for(|| {
        query
            .get_fault("hvac", "hvac.blower.speed_sensor_mismatch")
            .is_ok_and(|(fault, env)| {
                fault
                    .typed_status
                    .is_some_and(|s| s.test_failed == Some(true))
                    && env.get("application").is_some_and(|v| v == "cpp_reporter")
                    && env.get("temperature").is_some_and(|v| v == "42")
            })
    });
    success(
        Command::new(example)
            .arg(&catalog_path)
            .args(["hvac.blower.speed_sensor_mismatch", "passed"])
            .output()
            .unwrap(),
    );
    wait_for(|| {
        query
            .get_fault("hvac", "hvac.blower.speed_sensor_mismatch")
            .is_ok_and(|(fault, _)| {
                fault
                    .typed_status
                    .is_some_and(|s| s.test_failed == Some(false) && s.confirmed_dtc == Some(false))
            })
    });
}
