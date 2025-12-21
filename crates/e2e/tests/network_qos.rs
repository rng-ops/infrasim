use infrasim_common::traffic_shaper::{ShapingDecision, TrafficShaper};
use infrasim_common::types::QosProfileSpec;
use std::time::{Duration, Instant};

/// Network QoS Effect Test
///
/// Validates an observable latency effect by measuring a baseline delay,
/// then applying a QoS latency profile via `TrafficShaper` and asserting
/// the observed delay increases by a meaningful amount.
///
/// This test validates *observable effect* of the userspace shaper, not OS-level
/// traffic control of actual network packets.
#[tokio::test]
async fn qos_latency_increases_observed_delay() {
    let baseline_start = Instant::now();
    tokio::time::sleep(Duration::from_millis(10)).await;
    let baseline = baseline_start.elapsed();

    let shaper = TrafficShaper::new(QosProfileSpec {
        latency_ms: 60,
        ..Default::default()
    });

    let start = Instant::now();
    let decision = shaper.shape_packet(128).await;

    match decision {
        ShapingDecision::Delay(d) => {
            tokio::time::sleep(d).await;
        }
        ShapingDecision::SendPadded { delay, .. } => {
            tokio::time::sleep(delay).await;
        }
        ShapingDecision::Send => {
            panic!("expected a latency-inducing shaping decision");
        }
        ShapingDecision::Drop => {
            panic!("unexpected drop decision for latency-only profile");
        }
    }

    let shaped = start.elapsed();

    assert!(
        shaped > baseline + Duration::from_millis(30),
        "expected shaped latency to exceed baseline meaningfully (baseline={:?}, shaped={:?})",
        baseline,
        shaped
    );
}
