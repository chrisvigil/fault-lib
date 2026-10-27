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

use alloc::{string::String, vec::Vec};

use common::{
    fault::{LifecyclePhase, LifecycleStage},
    sink_error::SinkError,
    types::{MetadataVec, ShortString, to_static_short_string},
};
use fault_lib::{api::InitError, reporter::ReporterError};

use crate::types::{
    FL_ALREADY_INITIALIZED, FL_CATALOG_ERROR, FL_INVALID_ARGUMENT, FL_NOT_INITIALIZED,
    FL_PERMISSION_DENIED, FL_PHASE_INIT, FL_PHASE_RESUME, FL_PHASE_RUNNING, FL_PHASE_SHUTDOWN,
    FL_PHASE_SUSPEND, FL_QUEUE_FULL, FL_RATE_LIMITED, FL_STAGE_FAILED, FL_STAGE_NOT_TESTED,
    FL_STAGE_PASSED, FL_STAGE_PREFAILED, FL_STAGE_PREPASSED, FL_TIMEOUT, FL_TRANSPORT_ERROR,
    FL_UNKNOWN_FAULT, FlStatus,
};

#[derive(Debug)]
pub(crate) struct Error {
    pub status: FlStatus,
    pub message: String,
}

impl Error {
    pub fn new(status: FlStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(FL_INVALID_ARGUMENT, message)
    }
}

impl From<SinkError> for Error {
    fn from(error: SinkError) -> Self {
        let status = match &error {
            SinkError::Timeout => FL_TIMEOUT,
            SinkError::QueueFull => FL_QUEUE_FULL,
            SinkError::PermissionDenied => FL_PERMISSION_DENIED,
            SinkError::RateLimited => FL_RATE_LIMITED,
            SinkError::BadDescriptor(_) | SinkError::InvalidServiceName => FL_INVALID_ARGUMENT,
            _ => FL_TRANSPORT_ERROR,
        };
        Self::new(status, error.to_string())
    }
}

impl From<InitError> for Error {
    fn from(error: InitError) -> Self {
        match error {
            InitError::AlreadyInitialized => Self::new(FL_ALREADY_INITIALIZED, error.to_string()),
            InitError::CatalogVerification(SinkError::Other(message)) => {
                Self::new(FL_CATALOG_ERROR, message)
            }
            InitError::CatalogVerification(sink) => sink.into(),
            _ => Self::new(FL_TRANSPORT_ERROR, error.to_string()),
        }
    }
}

impl From<ReporterError> for Error {
    fn from(error: ReporterError) -> Self {
        if let ReporterError::SinkNotAvailable(sink) = error {
            return sink.into();
        }
        let status = match &error {
            ReporterError::FaultIdNotFound(_) => FL_UNKNOWN_FAULT,
            ReporterError::CatalogNotInitialized => FL_NOT_INITIALIZED,
            _ => FL_TRANSPORT_ERROR,
        };
        Self::new(status, error.to_string())
    }
}

pub(crate) fn stage(value: u32) -> Result<LifecycleStage, Error> {
    match value {
        FL_STAGE_NOT_TESTED => Ok(LifecycleStage::NotTested),
        FL_STAGE_PREFAILED => Ok(LifecycleStage::PreFailed),
        FL_STAGE_FAILED => Ok(LifecycleStage::Failed),
        FL_STAGE_PREPASSED => Ok(LifecycleStage::PrePassed),
        FL_STAGE_PASSED => Ok(LifecycleStage::Passed),
        _ => Err(Error::invalid("unknown lifecycle stage")),
    }
}

pub(crate) fn phase(value: u32) -> Result<LifecyclePhase, Error> {
    match value {
        FL_PHASE_INIT => Ok(LifecyclePhase::Init),
        FL_PHASE_RUNNING => Ok(LifecyclePhase::Running),
        FL_PHASE_SUSPEND => Ok(LifecyclePhase::Suspend),
        FL_PHASE_RESUME => Ok(LifecyclePhase::Resume),
        FL_PHASE_SHUTDOWN => Ok(LifecyclePhase::Shutdown),
        _ => Err(Error::invalid("unknown lifecycle phase")),
    }
}

pub(crate) fn short(value: &str) -> Result<ShortString, Error> {
    to_static_short_string(value)
        .map_err(|e| Error::invalid(alloc::format!("invalid short string (max 64 bytes): {e}")))
}

pub(crate) fn merge_metadata(
    defaults: &MetadataVec,
    overrides: &MetadataVec,
) -> Result<MetadataVec, Error> {
    let mut merged: Vec<_> = defaults.iter().copied().collect();
    for (key, value) in overrides.iter() {
        if let Some((_, old)) = merged.iter_mut().find(|(candidate, _)| candidate == key) {
            *old = *value;
        } else {
            merged.push((*key, *value));
        }
    }
    MetadataVec::try_from(merged.as_slice())
        .map_err(|_| Error::invalid("merged metadata exceeds eight entries"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn lifecycle_values_and_limits() {
        for (value, expected) in [
            (FL_STAGE_NOT_TESTED, LifecycleStage::NotTested),
            (FL_STAGE_PREFAILED, LifecycleStage::PreFailed),
            (FL_STAGE_FAILED, LifecycleStage::Failed),
            (FL_STAGE_PREPASSED, LifecycleStage::PrePassed),
            (FL_STAGE_PASSED, LifecycleStage::Passed),
        ] {
            assert_eq!(stage(value).unwrap(), expected);
        }
        for (value, expected) in [
            (FL_PHASE_INIT, LifecyclePhase::Init),
            (FL_PHASE_RUNNING, LifecyclePhase::Running),
            (FL_PHASE_SUSPEND, LifecyclePhase::Suspend),
            (FL_PHASE_RESUME, LifecyclePhase::Resume),
            (FL_PHASE_SHUTDOWN, LifecyclePhase::Shutdown),
        ] {
            assert_eq!(phase(value).unwrap(), expected);
        }
        assert_eq!(stage(5).err().unwrap().status, FL_INVALID_ARGUMENT);
        assert!(phase(u32::MAX).is_err());
        assert!(short(&"x".repeat(64)).is_ok());
        assert!(short(&"x".repeat(65)).is_err());
        assert!(short("non-ASCII: é").is_err());
        assert!(short("embedded\0NUL").is_err());
    }

    #[test]
    fn metadata_overrides_defaults_without_modifying_them() {
        let defaults =
            MetadataVec::try_from(&[(short("key").unwrap(), short("old").unwrap())][..]).unwrap();
        let overrides =
            MetadataVec::try_from(&[(short("key").unwrap(), short("new").unwrap())][..]).unwrap();
        let result = merge_metadata(&defaults, &overrides).unwrap();
        assert_eq!(result.iter().next().unwrap().1, short("new").unwrap());
        assert_eq!(defaults.iter().next().unwrap().1, short("old").unwrap());
    }

    #[test]
    fn merged_metadata_capacity_is_checked_after_overrides() {
        let entries: Vec<_> = (0..8)
            .map(|i| (short(&i.to_string()).unwrap(), short("v").unwrap()))
            .collect();
        let defaults = MetadataVec::try_from(entries.as_slice()).unwrap();
        assert!(merge_metadata(&defaults, &defaults).is_ok());
        let extra =
            MetadataVec::try_from(&[(short("extra").unwrap(), short("v").unwrap())][..]).unwrap();
        assert!(merge_metadata(&defaults, &extra).is_err());
        assert_eq!(merge_metadata(&MetadataVec::new(), &extra).unwrap(), extra);
    }

    #[test]
    fn sink_errors_keep_actionable_statuses() {
        for (error, status) in [
            (SinkError::Timeout, FL_TIMEOUT),
            (SinkError::QueueFull, FL_QUEUE_FULL),
            (SinkError::PermissionDenied, FL_PERMISSION_DENIED),
            (SinkError::RateLimited, FL_RATE_LIMITED),
            (SinkError::TransportDown, FL_TRANSPORT_ERROR),
            (SinkError::InvalidServiceName, FL_INVALID_ARGUMENT),
            (SinkError::BadDescriptor("bad".into()), FL_INVALID_ARGUMENT),
            (SinkError::Other("other".into()), FL_TRANSPORT_ERROR),
        ] {
            assert_eq!(Error::from(error).status, status);
        }
        assert_eq!(
            Error::from(InitError::AlreadyInitialized).status,
            FL_ALREADY_INITIALIZED
        );
        assert_eq!(
            Error::from(InitError::CatalogVerification(SinkError::Timeout)).status,
            FL_TIMEOUT
        );
        assert_eq!(
            Error::from(ReporterError::CatalogNotInitialized).status,
            FL_NOT_INITIALIZED
        );
        assert_eq!(
            Error::from(ReporterError::SinkNotAvailable(SinkError::TransportDown)).status,
            FL_TRANSPORT_ERROR
        );
    }
}
