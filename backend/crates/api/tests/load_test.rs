//! Load testing for BillForge API
//!
//! Run with: cargo test --test load_test -- --ignored --nocapture
//!
//! This test requires a running server at http://localhost:8080

use reqwest::Client;
use std::time::{Duration, Instant};

const BASE_URL: &str = "http://localhost:8080";
const CONCURRENT_REQUESTS: usize = 10;
const TOTAL_REQUESTS: usize = 100;

#[tokio::test]
#[ignore]
async fn load_test_health_endpoint() {
    let client = Client::new();
    let mut latencies = Vec::new();
    let mut errors = 0;

    println!("\n🚀 Starting load test: Health Endpoint");
    println!("Target: {} requests", TOTAL_REQUESTS);
    println!("Concurrency: {} requests", CONCURRENT_REQUESTS);

    let start = Instant::now();

    for _ in 0..(TOTAL_REQUESTS / CONCURRENT_REQUESTS) {
        let mut handles = Vec::new();

        for _ in 0..CONCURRENT_REQUESTS {
            let client = client.clone();
            let handle = tokio::spawn(async move {
                let req_start = Instant::now();
                let result = client.get(format!("{}/health", BASE_URL)).send().await;
                let latency = req_start.elapsed();

                match result {
                    Ok(resp) if resp.status().is_success() => Ok(latency),
                    Ok(resp) => Err(format!("HTTP {}", resp.status())),
                    Err(e) => Err(e.to_string()),
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            match handle.await.unwrap() {
                Ok(latency) => latencies.push(latency),
                Err(e) => {
                    errors += 1;
                    eprintln!("Error: {}", e);
                }
            }
        }
    }

    let total_time = start.elapsed();

    // Calculate statistics
    latencies.sort();
    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];

    println!("\n📊 Load Test Results:");
    println!("  Total Time: {:.2?}", total_time);
    println!(
        "  Requests/sec: {:.2}",
        TOTAL_REQUESTS as f64 / total_time.as_secs_f64()
    );
    println!(
        "  Errors: {}/{} ({:.1}%)",
        errors,
        TOTAL_REQUESTS,
        (errors as f64 / TOTAL_REQUESTS as f64) * 100.0
    );
    println!("\n⏱️  Latency Statistics:");
    println!("  Min:    {:?}", min);
    println!("  Avg:    {:?}", avg_latency);
    println!("  P50:    {:?}", p50);
    println!("  P95:    {:?}", p95);
    println!("  P99:    {:?}", p99);
    println!("  Max:    {:?}", max);

    // Verify SLA
    let success_rate = 1.0 - (errors as f64 / TOTAL_REQUESTS as f64);
    assert!(
        success_rate > 0.95,
        "Success rate {:.1}% < 95%",
        success_rate * 100.0
    );
    assert!(
        p95 < Duration::from_millis(200),
        "P95 latency {:?} > 200ms",
        p95
    );
}

#[tokio::test]
#[ignore]
async fn load_test_liveness_endpoint() {
    let client = Client::new();
    let mut latencies = Vec::new();

    println!("\n🚀 Starting load test: Liveness Endpoint");

    let start = Instant::now();

    for _ in 0..TOTAL_REQUESTS {
        let req_start = Instant::now();
        let resp = client
            .get(format!("{}/health/live", BASE_URL))
            .send()
            .await
            .expect("Request failed");

        assert!(
            resp.status().is_success(),
            "Unexpected status: {}",
            resp.status()
        );
        latencies.push(req_start.elapsed());
    }

    let total_time = start.elapsed();
    latencies.sort();

    println!("\n📊 Results:");
    println!(
        "  Total: {:.2?} ({:.0} req/s)",
        total_time,
        TOTAL_REQUESTS as f64 / total_time.as_secs_f64()
    );
    println!(
        "  P95: {:?}",
        latencies[(latencies.len() as f64 * 0.95) as usize]
    );

    // Verify P95 < 10ms
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    assert!(
        p95 < Duration::from_millis(10),
        "P95 latency {:?} > 10ms",
        p95
    );
}

#[tokio::test]
#[ignore]
async fn load_test_dashboard_metrics() {
    let client = Client::new();
    let mut latencies = Vec::new();
    let mut errors = 0;

    println!("\n🚀 Starting load test: Dashboard Metrics Endpoint");
    println!("Note: This test will fail without authentication token");

    let start = Instant::now();

    for _ in 0..TOTAL_REQUESTS {
        let req_start = Instant::now();
        let resp = client
            .get(format!("{}/api/v1/dashboard/metrics", BASE_URL))
            .send()
            .await;

        match resp {
            Ok(resp) => {
                // Expect 401 without auth token
                if resp.status().as_u16() == 401 {
                    latencies.push(req_start.elapsed());
                } else {
                    errors += 1;
                    eprintln!("Unexpected status: {}", resp.status());
                }
            }
            Err(e) => {
                errors += 1;
                eprintln!("Error: {}", e);
            }
        }
    }

    let total_time = start.elapsed();

    println!("\n📊 Results:");
    println!("  Total: {:.2?}", total_time);
    println!("  Success: {}/{}", TOTAL_REQUESTS - errors, TOTAL_REQUESTS);

    // Verify endpoint responds quickly even with auth failure
    if !latencies.is_empty() {
        latencies.sort();
        let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
        println!("  P95: {:?}", p95);
        assert!(
            p95 < Duration::from_millis(100),
            "P95 latency {:?} > 100ms",
            p95
        );
    }
}

#[tokio::test]
#[ignore]
async fn load_test_concurrent_requests() {
    let client = Client::new();

    println!("\n🚀 Starting concurrency test: 50 simultaneous requests");

    let mut handles = Vec::new();

    for i in 0..50 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let resp = client
                .get(format!("{}/health", BASE_URL))
                .send()
                .await
                .expect("Request failed");

            assert!(resp.status().is_success());
            i
        });
        handles.push(handle);
    }

    let start = Instant::now();

    let mut completed = 0;
    for handle in handles {
        handle.await.unwrap();
        completed += 1;
    }

    let total_time = start.elapsed();

    println!("\n📊 Results:");
    println!("  Completed: {}/50", completed);
    println!("  Total Time: {:.2?}", total_time);
    println!("  Avg per request: {:.2?}", total_time / completed);

    assert_eq!(completed, 50, "Not all requests completed");
    assert!(
        total_time < Duration::from_secs(5),
        "50 concurrent requests took >5s"
    );
}
