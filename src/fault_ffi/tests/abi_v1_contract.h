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

#ifndef FAULT_FFI_ABI_V1_CONTRACT_H
#define FAULT_FFI_ABI_V1_CONTRACT_H

#include "fault_ffi.h"

/* Frozen v1 baseline, independent of header generation. Changes require ABI
 * review, not automatic regeneration. Use the target C layout, not LP64 sizes. */
struct abi_v1_fl_string {
    const uint8_t *data;
    size_t len;
};

struct abi_v1_fl_metadata {
    struct abi_v1_fl_string key;
    struct abi_v1_fl_string value;
};

struct abi_v1_fl_reporter_config {
    size_t struct_size;
    struct abi_v1_fl_string source_entity;
    struct abi_v1_fl_string source_ecu;
    struct abi_v1_fl_string source_domain;
    struct abi_v1_fl_string source_sw_component;
    struct abi_v1_fl_string source_instance;
    uint32_t lifecycle_phase;
    const struct abi_v1_fl_metadata *default_metadata;
    size_t default_metadata_len;
};

#define ABI_V1_LAYOUT(type) \
    _Static_assert(sizeof(type) == sizeof(struct abi_v1_##type), #type " size"); \
    _Static_assert(_Alignof(type) == _Alignof(struct abi_v1_##type), #type " alignment")
#define ABI_V1_FIELD(type, field, expected) \
    _Static_assert(offsetof(type, field) == offsetof(struct abi_v1_##type, field), \
                   #type "." #field " offset"); \
    _Static_assert(_Generic(((type *)0)->field, expected: 1, default: 0), \
                   #type "." #field " type")
#define ABI_V1_CONSTANT(name, value) _Static_assert((name) == (value), #name " value")
#define ABI_V1_SIGNATURE(name, ...) \
    _Static_assert(_Generic(&(name), __VA_ARGS__: 1, default: 0), #name " signature")

_Static_assert(_Generic((fl_status)0, int32_t: 1, default: 0), "fl_status type");
ABI_V1_LAYOUT(fl_string);
ABI_V1_FIELD(fl_string, data, const uint8_t *);
ABI_V1_FIELD(fl_string, len, size_t);
ABI_V1_LAYOUT(fl_metadata);
ABI_V1_FIELD(fl_metadata, key, fl_string);
ABI_V1_FIELD(fl_metadata, value, fl_string);
ABI_V1_LAYOUT(fl_reporter_config);
ABI_V1_FIELD(fl_reporter_config, struct_size, size_t);
ABI_V1_FIELD(fl_reporter_config, source_entity, fl_string);
ABI_V1_FIELD(fl_reporter_config, source_ecu, fl_string);
ABI_V1_FIELD(fl_reporter_config, source_domain, fl_string);
ABI_V1_FIELD(fl_reporter_config, source_sw_component, fl_string);
ABI_V1_FIELD(fl_reporter_config, source_instance, fl_string);
ABI_V1_FIELD(fl_reporter_config, lifecycle_phase, uint32_t);
ABI_V1_FIELD(fl_reporter_config, default_metadata, const fl_metadata *);
ABI_V1_FIELD(fl_reporter_config, default_metadata_len, size_t);

ABI_V1_CONSTANT(FL_ABI_VERSION, 1);
ABI_V1_CONSTANT(FL_OK, 0);
ABI_V1_CONSTANT(FL_INVALID_ARGUMENT, 1);
ABI_V1_CONSTANT(FL_ALREADY_INITIALIZED, 2);
ABI_V1_CONSTANT(FL_UNKNOWN_FAULT, 3);
ABI_V1_CONSTANT(FL_CATALOG_ERROR, 4);
ABI_V1_CONSTANT(FL_TRANSPORT_ERROR, 5);
ABI_V1_CONSTANT(FL_TIMEOUT, 6);
ABI_V1_CONSTANT(FL_QUEUE_FULL, 7);
ABI_V1_CONSTANT(FL_PERMISSION_DENIED, 8);
ABI_V1_CONSTANT(FL_RATE_LIMITED, 9);
ABI_V1_CONSTANT(FL_INTERNAL_ERROR, 10);
ABI_V1_CONSTANT(FL_BUFFER_TOO_SMALL, 11);
ABI_V1_CONSTANT(FL_NOT_INITIALIZED, 12);
ABI_V1_CONSTANT(FL_STAGE_NOT_TESTED, 0);
ABI_V1_CONSTANT(FL_STAGE_PREFAILED, 1);
ABI_V1_CONSTANT(FL_STAGE_FAILED, 2);
ABI_V1_CONSTANT(FL_STAGE_PREPASSED, 3);
ABI_V1_CONSTANT(FL_STAGE_PASSED, 4);
ABI_V1_CONSTANT(FL_PHASE_INIT, 0);
ABI_V1_CONSTANT(FL_PHASE_RUNNING, 1);
ABI_V1_CONSTANT(FL_PHASE_SUSPEND, 2);
ABI_V1_CONSTANT(FL_PHASE_RESUME, 3);
ABI_V1_CONSTANT(FL_PHASE_SHUTDOWN, 4);
ABI_V1_CONSTANT(FL_SHORT_STRING_CAPACITY, 64);
ABI_V1_CONSTANT(FL_PATH_CAPACITY, 128);
ABI_V1_CONSTANT(FL_METADATA_CAPACITY, 8);

ABI_V1_SIGNATURE(fl_abi_version, uint32_t (*)(void));
ABI_V1_SIGNATURE(fl_context_create, fl_status (*)(fl_string, fl_context **));
ABI_V1_SIGNATURE(fl_context_catalog_id,
                 fl_status (*)(const fl_context *, uint8_t *, size_t, size_t *));
ABI_V1_SIGNATURE(fl_context_destroy, fl_status (*)(fl_context *));
ABI_V1_SIGNATURE(fl_reporter_create_numeric,
                 fl_status (*)(const fl_context *, uint32_t, const fl_reporter_config *, fl_reporter **));
ABI_V1_SIGNATURE(fl_reporter_create_text,
                 fl_status (*)(const fl_context *, fl_string, const fl_reporter_config *, fl_reporter **));
ABI_V1_SIGNATURE(fl_reporter_create_uuid,
                 fl_status (*)(const fl_context *, const uint8_t *, const fl_reporter_config *, fl_reporter **));
ABI_V1_SIGNATURE(fl_reporter_publish,
                 fl_status (*)(fl_reporter *, fl_string, uint32_t, const fl_metadata *, size_t));
ABI_V1_SIGNATURE(fl_reporter_destroy, fl_status (*)(fl_reporter *));
ABI_V1_SIGNATURE(fl_last_error_message, fl_status (*)(uint8_t *, size_t, size_t *));

#undef ABI_V1_LAYOUT
#undef ABI_V1_FIELD
#undef ABI_V1_CONSTANT
#undef ABI_V1_SIGNATURE

#endif /* FAULT_FFI_ABI_V1_CONTRACT_H */
