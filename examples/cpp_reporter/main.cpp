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

#include <fstream>
#include <chrono>
#include <iostream>
#include <iterator>
#include <stdexcept>
#include <thread>

namespace {
void check(fl_status status) {
    if (status != FL_OK) {
        // Capture before RAII cleanup, which is itself a successful ABI operation.
        throw std::runtime_error("fault library status " + std::to_string(status) + ": " +
                                 fault::last_error());
    }
}
} // namespace

int main(int argc, char **argv) {
    if (argc < 2 || argc > 4) {
        std::cerr << "Usage: " << argv[0]
                  << " <catalog.json> [text-fault-id] [failed|passed|demo]\n";
        return 2;
    }
    try {
        if (fl_abi_version() != FL_ABI_VERSION) {
            throw std::runtime_error("incompatible fault library ABI");
        }
        std::ifstream input(argv[1]);
        if (!input) {
            throw std::runtime_error("cannot open catalog");
        }
        const std::string json((std::istreambuf_iterator<char>(input)), {});
        const std::string_view id = argc >= 3 ? argv[2] : "hvac.blower.speed_sensor_mismatch";
        const std::string_view mode = argc >= 4 ? argv[3] : "demo";
        if (mode != "failed" && mode != "passed" && mode != "demo") {
            throw std::runtime_error("stage must be failed, passed, or demo");
        }

        fault::Context context;
        check(fault::Context::create(json, context));
        std::string path;
        check(context.catalog_id(path));

        const auto defaults = fault::metadata("application", "cpp_reporter");
        auto config = fault::configuration("cpp_reporter");
        config.source_ecu = fault::string("ECU-A");
        config.default_metadata = &defaults;
        config.default_metadata_len = 1;

        fault::Reporter reporter;
        check(fault::Reporter::text(context, id, config, reporter));
        // The reporter owns an API reference; the context need not outlive it.
        context.reset();

        if (mode != "passed") {
            check(reporter.publish(path, fault::Stage::Failed, {fault::metadata("temperature", "42")}));
        }
        if (mode != "failed") {
            // The HVAC text fault has a 100ms manager-side cooldown.
            std::this_thread::sleep_for(std::chrono::milliseconds(200));
            check(reporter.publish(path, fault::Stage::PrePassed));
            std::this_thread::sleep_for(std::chrono::milliseconds(200));
            check(reporter.publish(path, fault::Stage::Passed));
        }
        std::cout << "Reports accepted locally for " << path << '/' << id
                  << "; delivery is asynchronous and may be suppressed by debounce.\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
