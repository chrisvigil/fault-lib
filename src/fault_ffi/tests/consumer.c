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

#include <stdio.h>
#include <string.h>

#define CHECK(condition) do { if (!(condition)) { \
    fprintf(stderr, "check failed at line %d: %s\n", __LINE__, #condition); return 1; \
} } while (0)

int main(void) {
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
    return 0;
}
