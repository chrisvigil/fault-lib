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

#include "fault_ffi.hpp"

#include <iostream>
#include <fstream>
#include <iterator>
#include <type_traits>

static_assert(!std::is_copy_constructible_v<fault::Context>);
static_assert(!std::is_copy_constructible_v<fault::Reporter>);
static_assert(std::is_nothrow_move_constructible_v<fault::Context>);
static_assert(std::is_nothrow_move_assignable_v<fault::Reporter>);
static_assert(std::is_nothrow_destructible_v<fault::Reporter>);

int main(int argc, char **argv) {
    fault::Context context;
    if (fault::Context::create("not JSON", context) != FL_CATALOG_ERROR ||
        fault::last_error().empty()) {
        return 1;
    }
    fault::Context moved(std::move(context));
    context = std::move(moved);
    fault::Reporter reporter;
    auto config = fault::configuration("test");
    if (fault::Reporter::numeric(context, 42, config, reporter) != FL_INVALID_ARGUMENT) {
        return 1;
    }
    fault::Reporter moved_reporter(std::move(reporter));
    reporter = std::move(moved_reporter);
    if (reporter.publish("test", fault::Stage::Failed) != FL_INVALID_ARGUMENT) {
        return 1;
    }
    reporter.reset();
    context.reset();
    if (argc == 2) {
        std::ifstream file(argv[1]);
        const std::string json((std::istreambuf_iterator<char>(file)), {});
        if (fault::Context::create(json, context) != FL_OK) {
            std::cerr << fault::last_error() << '\n';
            return 1;
        }
        moved = std::move(context);
        if (fault::Reporter::text(moved, "hvac.blower.speed_sensor_mismatch", config, reporter) != FL_OK) {
            return 1;
        }
        fault::Reporter second;
        if (fault::Reporter::numeric(moved, 0x7001, config, second) != FL_OK) {
            return 1;
        }
        std::array<uint8_t, 16> uuid;
        uuid.fill(7);
        fault::Reporter uuid_reporter;
        if (fault::Reporter::uuid(moved, uuid, config, uuid_reporter) != FL_OK) {
            return 1;
        }
        // Move assignment must release a live destination, not leak or double-free it.
        second = std::move(reporter);
        moved_reporter = std::move(second);
        moved.reset();
        if (moved_reporter.publish("hvac", fault::Stage::Failed,
                                  {fault::metadata("application", "cpp_consumer")}) != FL_OK) {
            return 1;
        }
        moved_reporter.reset();
        if (fault::Context::create(json, context) != FL_ALREADY_INITIALIZED) {
            return 1;
        }
    }
    return fl_abi_version() == FL_ABI_VERSION ? 0 : 1;
}
