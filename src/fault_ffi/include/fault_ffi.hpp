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

#ifndef FAULT_FFI_HPP
#define FAULT_FFI_HPP

#include "fault_ffi.h"

#include <array>
#include <initializer_list>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace fault {

inline fl_string string(std::string_view value) noexcept {
    return {reinterpret_cast<const uint8_t *>(value.data()), value.size()};
}

inline fl_metadata metadata(std::string_view key, std::string_view value) noexcept {
    return {string(key), string(value)};
}

// String views are borrowed until reporter creation; do not retain dangling views.
inline fl_reporter_config configuration(std::string_view entity,
                                        uint32_t phase = FL_PHASE_RUNNING) noexcept {
    fl_reporter_config config{};
    config.struct_size = sizeof(config);
    config.source_entity = string(entity);
    config.lifecycle_phase = phase;
    return config;
}

inline std::string last_error() {
    size_t required = 0;
    if (fl_last_error_message(nullptr, 0, &required) != FL_BUFFER_TOO_SMALL) {
        return "unable to retrieve fault library error";
    }
    std::vector<uint8_t> buffer(required);
    if (fl_last_error_message(buffer.data(), buffer.size(), &required) != FL_OK) {
        return "unable to retrieve fault library error";
    }
    return {reinterpret_cast<const char *>(buffer.data()), required - 1};
}

enum class Stage : uint32_t {
    NotTested = FL_STAGE_NOT_TESTED,
    PreFailed = FL_STAGE_PREFAILED,
    Failed = FL_STAGE_FAILED,
    PrePassed = FL_STAGE_PREPASSED,
    Passed = FL_STAGE_PASSED,
};

class Reporter;

// One successful initialization per process, even after all objects are destroyed.
class Context {
public:
    Context() noexcept = default;
    ~Context() noexcept { reset(); }
    Context(const Context &) = delete;
    Context &operator=(const Context &) = delete;
    Context(Context &&other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    Context &operator=(Context &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    static fl_status create(std::string_view catalog_json, Context &out) noexcept {
        fl_context *handle = nullptr;
        const auto status = fl_context_create(string(catalog_json), &handle);
        if (status == FL_OK) {
            out.reset();
            out.handle_ = handle;
        }
        return status;
    }

    fl_status catalog_id(std::string &out) const {
        size_t required = 0;
        const auto status = fl_context_catalog_id(handle_, nullptr, 0, &required);
        if (status != FL_BUFFER_TOO_SMALL) {
            return status;
        }
        std::vector<uint8_t> buffer(required);
        const auto copied = fl_context_catalog_id(handle_, buffer.data(), buffer.size(), &required);
        if (copied == FL_OK) {
            out.assign(reinterpret_cast<const char *>(buffer.data()), required - 1);
        }
        return copied;
    }

    void reset() noexcept {
        if (auto *handle = std::exchange(handle_, nullptr)) {
            (void)fl_context_destroy(handle);
        }
    }

private:
    friend class Reporter;
    fl_context *handle_ = nullptr;
};

// Different reporters may be used independently. Serialize use/move/reset of
// each individual reporter. Destruction does not promise a delivery flush.
class Reporter {
public:
    Reporter() noexcept = default;
    ~Reporter() noexcept { reset(); }
    Reporter(const Reporter &) = delete;
    Reporter &operator=(const Reporter &) = delete;
    Reporter(Reporter &&other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    Reporter &operator=(Reporter &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    static fl_status numeric(const Context &context, uint32_t id,
                             const fl_reporter_config &config, Reporter &out) noexcept {
        fl_reporter *handle = nullptr;
        const auto status = fl_reporter_create_numeric(context.handle_, id, &config, &handle);
        return adopt(status, handle, out);
    }

    static fl_status text(const Context &context, std::string_view id,
                          const fl_reporter_config &config, Reporter &out) noexcept {
        fl_reporter *handle = nullptr;
        const auto status = fl_reporter_create_text(context.handle_, string(id), &config, &handle);
        return adopt(status, handle, out);
    }

    static fl_status uuid(const Context &context, const std::array<uint8_t, 16> &id,
                          const fl_reporter_config &config, Reporter &out) noexcept {
        fl_reporter *handle = nullptr;
        const auto status = fl_reporter_create_uuid(context.handle_, id.data(), &config, &handle);
        return adopt(status, handle, out);
    }

    fl_status publish(std::string_view path, Stage stage,
                      std::initializer_list<fl_metadata> entries = {}) noexcept {
        return fl_reporter_publish(handle_, string(path), static_cast<uint32_t>(stage),
                                   entries.begin(), entries.size());
    }

    void reset() noexcept {
        if (auto *handle = std::exchange(handle_, nullptr)) {
            (void)fl_reporter_destroy(handle);
        }
    }

private:
    static fl_status adopt(fl_status status, fl_reporter *handle, Reporter &out) noexcept {
        if (status == FL_OK) {
            out.reset();
            out.handle_ = handle;
        }
        return status;
    }

    fl_reporter *handle_ = nullptr;
};

} // namespace fault

#endif // FAULT_FFI_HPP
