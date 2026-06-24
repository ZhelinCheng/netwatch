//! DNS 探测器。
//!
use std::time::{Duration, Instant};

use hickory_resolver::{
    TokioResolver,
    config::{GOOGLE, ResolverConfig},
    net::NetError,
    net::runtime::TokioRuntimeProvider,
    proto::rr::{RData, RecordType},
};
use tokio::time;

use crate::{
    domain::{
        check::CheckResult,
        monitor::{DnsRecordType, Monitor},
    },
    error::AppError,
    probes::observation::{ProbeObservation, failure_reason},
};

/// 解析目标域名并记录耗时，可选校验期望 IP/值。
pub async fn probe(monitor: &Monitor, timeout: Duration) -> Result<CheckResult, AppError> {
    let started = Instant::now();
    let target = dns_target(&monitor.target);
    let record_type = monitor
        .config
        .dns_record
        .clone()
        .unwrap_or(DnsRecordType::A);
    tracing::debug!(
        monitor_id = monitor.id,
        target = %target,
        record_type = record_type.as_str(),
        timeout_ms = timeout.as_millis(),
        "starting dns probe"
    );

    let resolver = resolver().map_err(anyhow::Error::from)?;
    let values = time::timeout(timeout, lookup_records(&resolver, &target, &record_type))
        .await
        .map_err(|_| AppError::BadRequest("dns lookup timed out".to_string()))?
        .map_err(anyhow::Error::from)?;
    let latency_us = started.elapsed().as_micros() as u64;
    let mut observation = ProbeObservation::new();
    observation.dns_answers = values;
    let answer_count = observation.dns_answers.len();

    let failure_reason = failure_reason(monitor, &observation);
    tracing::debug!(
        monitor_id = monitor.id,
        record_type = record_type.as_str(),
        answer_count = answer_count,
        latency_us = latency_us,
        success = failure_reason.is_none(),
        "dns probe observed answers"
    );

    if let Some(message) = failure_reason {
        Ok(CheckResult::failed_with_message(
            monitor.id,
            Some(latency_us),
            message,
        ))
    } else {
        Ok(CheckResult::success(monitor.id, latency_us))
    }
}

fn resolver() -> Result<TokioResolver, NetError> {
    let builder = TokioResolver::builder_tokio().unwrap_or_else(|error| {
        tracing::debug!(
            error = %error,
            "failed to load system dns config, falling back to default resolver"
        );
        TokioResolver::builder_with_config(
            ResolverConfig::udp_and_tcp(&GOOGLE),
            TokioRuntimeProvider::default(),
        )
    });
    builder.build()
}

async fn lookup_records(
    resolver: &TokioResolver,
    target: &str,
    record_type: &DnsRecordType,
) -> Result<Vec<String>, NetError> {
    let lookup = resolver.lookup(target, to_record_type(record_type)).await?;
    Ok(lookup
        .answers()
        .iter()
        .map(|record| record_value(&record.data))
        .collect())
}

fn dns_target(target: &str) -> String {
    target
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(target)
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn to_record_type(record_type: &DnsRecordType) -> RecordType {
    match record_type {
        DnsRecordType::A => RecordType::A,
        DnsRecordType::AAAA => RecordType::AAAA,
        DnsRecordType::CNAME => RecordType::CNAME,
        DnsRecordType::MX => RecordType::MX,
        DnsRecordType::TXT => RecordType::TXT,
        DnsRecordType::NS => RecordType::NS,
        DnsRecordType::SOA => RecordType::SOA,
        DnsRecordType::CAA => RecordType::CAA,
        DnsRecordType::SRV => RecordType::SRV,
    }
}

fn record_value(value: &RData) -> String {
    match value {
        RData::A(value) => value.0.to_string(),
        RData::AAAA(value) => value.0.to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::{
        check::CheckStatus,
        monitor::{DnsRecordType, Monitor, MonitorConfig, MonitorKind},
    };

    use super::*;

    #[tokio::test]
    async fn dns_probe_resolves_localhost_and_applies_answer_rule() {
        let now = Utc::now();
        let monitor = Monitor {
            id: 11,
            name: "dns".into(),
            kind: MonitorKind::Dns,
            target: "localhost".into(),
            config: MonitorConfig {
                dns_record: Some(DnsRecordType::A),
                expected_value: Some("127.0.0.1".into()),
                ..MonitorConfig::default()
            },
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        let result = probe(&monitor, std::time::Duration::from_secs(1))
            .await
            .unwrap();

        assert_eq!(result.status, CheckStatus::Success);
    }

    #[test]
    fn dns_target_strips_port_and_record_type_maps_to_dns_type() {
        assert_eq!(dns_target("example.com:5353"), "example.com");
        assert_eq!(to_record_type(&DnsRecordType::TXT), RecordType::TXT);
    }
}
