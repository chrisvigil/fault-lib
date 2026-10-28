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

#include "fault_ffi.h"
#include "abi_v1_contract.h"

#include <stdio.h>
#include <string.h>

#define CHECK(condition) do { if (!(condition)) { \
    fprintf(stderr, "check failed at line %d: %s\n", __LINE__, #condition); return 1; \
} } while (0)

static fl_string text(const char *value) {
    return (fl_string){(const uint8_t *)value, strlen(value)};
}

int main(int argc, char **argv) {
    fl_context *context = NULL;
    fl_reporter *reporter = NULL;
    size_t required = 0;
    uint8_t error[256];
    const uint8_t invalid_utf8[] = {0xff};
    CHECK(fl_abi_version() == FL_ABI_VERSION);
    CHECK(fl_context_create((fl_string){NULL, 1}, &context) == FL_INVALID_ARGUMENT);
    CHECK(context == NULL);
    CHECK(fl_context_create((fl_string){invalid_utf8, 1}, &context) == FL_INVALID_ARGUMENT);
    CHECK(fl_context_create((fl_string){NULL, 0}, &context) == FL_CATALOG_ERROR);
    CHECK(fl_last_error_message(NULL, 0, &required) == FL_BUFFER_TOO_SMALL);
    CHECK(required > 1 && required <= sizeof(error));
    memset(error, 42, sizeof(error));
    CHECK(fl_last_error_message(error, 1, &required) == FL_BUFFER_TOO_SMALL);
    CHECK(error[0] == 42);
    CHECK(fl_last_error_message(error, sizeof(error), &required) == FL_OK);
    CHECK(error[required - 1] == 0);
    CHECK(strstr((const char *)error, "JSON") != NULL);
    CHECK(fl_reporter_create_numeric(NULL, 42, NULL, &reporter) == FL_INVALID_ARGUMENT);
    CHECK(reporter == NULL);
    CHECK(fl_reporter_publish(NULL, (fl_string){NULL, 0}, FL_STAGE_FAILED, NULL, 0)
          == FL_INVALID_ARGUMENT);
    CHECK(fl_context_destroy(NULL) == FL_OK);
    CHECK(fl_reporter_destroy(NULL) == FL_OK);
    if (argc == 2) {
        /* The integration harness supplies a small, self-contained catalog. */
        uint8_t json[65536];
        FILE *file = fopen(argv[1], "rb");
        CHECK(file != NULL);
        const size_t length = fread(json, 1, sizeof(json), file);
        CHECK(!ferror(file) && feof(file));
        CHECK(fclose(file) == 0);
        CHECK(fl_context_create((fl_string){json, length}, &context) == FL_OK);
        fl_metadata defaults = {text("application"), text("c_consumer")};
        fl_reporter_config config = {0};
        config.struct_size = sizeof(config);
        config.source_entity = text("c_consumer");
        config.lifecycle_phase = FL_PHASE_RUNNING;
        config.default_metadata = &defaults;
        config.default_metadata_len = 1;
        CHECK(fl_reporter_create_text(context, text("hvac.blower.speed_sensor_mismatch"),
                                      &config, &reporter) == FL_OK);
        /* Input data is copied, and the reporter retains the Rust API owner. */
        memset(json, 0, length);
        defaults.value = text("changed");
        CHECK(fl_context_destroy(context) == FL_OK);
        CHECK(fl_reporter_publish(reporter, text("hvac"), FL_STAGE_FAILED, NULL, 0) == FL_OK);
        CHECK(fl_reporter_destroy(reporter) == FL_OK);
    }
    return 0;
}
