use super::labels::PipelineStage;
use super::latency::{percentile_from_hist_slice, record_hist_slice};
use super::registry::stage_latency_hists;
use std::fmt::Write;
use std::time::Duration;

/// records latency for a specific pipeline stage.
pub fn record_stage_latency(stage: PipelineStage, duration: Duration) {
    if let Some(hist) = stage_latency_hists().get(stage.as_index()) {
        record_hist_slice(hist, duration);
    }
}

pub(super) fn append_stage_latency_metrics(out: &mut String) {
    for stage in PipelineStage::ALL {
        let Some(hist) = stage_latency_hists().get(stage.as_index()) else {
            continue;
        };
        let p50 = percentile_from_hist_slice(hist, 50, 100);
        let p95 = percentile_from_hist_slice(hist, 95, 100);
        let p99 = percentile_from_hist_slice(hist, 99, 100);

        let _ = writeln!(
            out,
            "imagik_stage_latency_ms{{stage=\"{}\",quantile=\"0.50\"}} {}",
            stage.as_name(),
            p50
        );
        let _ = writeln!(
            out,
            "imagik_stage_latency_ms{{stage=\"{}\",quantile=\"0.95\"}} {}",
            stage.as_name(),
            p95
        );
        let _ = writeln!(
            out,
            "imagik_stage_latency_ms{{stage=\"{}\",quantile=\"0.99\"}} {}",
            stage.as_name(),
            p99
        );
    }
}
