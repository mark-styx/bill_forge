//! Prometheus metrics for BillForge Worker
//!
//! Mirrors OCR_PROVIDER_OUTCOME_TOTAL and OCR_FIRST_PASS_FIELD_CONFIDENCE
//! so the worker binary (separate process) can emit the same SLO/accuracy
//! telemetry as the API's synchronous OCR fallback.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_histogram_vec, CounterVec, Encoder, HistogramVec, TextEncoder,
};
use std::net::SocketAddr;

lazy_static! {
    /// OCR provider outcome counter (success/failure per provider).
    pub static ref OCR_PROVIDER_OUTCOME_TOTAL: CounterVec = register_counter_vec!(
        "billforge_ocr_provider_outcome_total",
        "OCR provider outcome counts",
        &["provider", "outcome"]
    ).unwrap();

    /// First-pass OCR field confidence histogram per field name.
    pub static ref OCR_FIRST_PASS_FIELD_CONFIDENCE: HistogramVec = register_histogram_vec!(
        "billforge_ocr_first_pass_field_confidence",
        "Confidence score distribution for first-pass OCR field extraction",
        &["field"],
        vec![0.1, 0.3, 0.5, 0.7, 0.8, 0.9, 0.95, 0.99, 1.0]
    ).unwrap();
}

fn ensure_metrics_registered() {
    OCR_PROVIDER_OUTCOME_TOTAL.with_label_values(&["unknown", "success"]);
    OCR_PROVIDER_OUTCOME_TOTAL.with_label_values(&["unknown", "failure"]);
    OCR_FIRST_PASS_FIELD_CONFIDENCE.with_label_values(&["unknown"]);
}

/// Export metrics in Prometheus text format.
pub fn export_metrics() -> String {
    ensure_metrics_registered();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Serve a minimal HTTP /metrics endpoint on the given address.
///
/// Uses a hand-rolled HTTP/1.1 response so the worker doesn't need to pull
/// in axum. Binds to `0.0.0.0:9091` by default (overridden via
/// `WORKER_METRICS_PORT` env var).
pub async fn serve_metrics(addr: SocketAddr) {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => {
            tracing::info!("Worker metrics endpoint listening on {}", addr);
            l
        }
        Err(e) => {
            tracing::error!("Failed to bind worker metrics endpoint on {}: {}", addr, e);
            return;
        }
    };

    loop {
        let stream = match listener.accept().await {
            Ok((s, _)) => s,
            Err(e) => {
                tracing::warn!("Metrics accept error: {}", e);
                continue;
            }
        };

        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let (mut rx, mut tx) = tokio::io::split(stream);

            // Read (and discard) the request — we only care about GET /metrics
            let mut buf = [0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut rx, &mut buf).await;

            let body = export_metrics();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = tx.write_all(response.as_bytes()).await;
            let _ = tx.shutdown().await;
        });
    }
}

/// Return the metrics listen address, reading `WORKER_METRICS_PORT` from env.
pub fn metrics_addr() -> SocketAddr {
    let port: u16 = std::env::var("WORKER_METRICS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9091);
    SocketAddr::from(([0, 0, 0, 0], port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_metrics_export() {
        OCR_PROVIDER_OUTCOME_TOTAL
            .with_label_values(&["tesseract", "success"])
            .inc();
        OCR_FIRST_PASS_FIELD_CONFIDENCE
            .with_label_values(&["invoice_number"])
            .observe(0.88);

        let metrics = export_metrics();

        assert!(
            metrics.contains("billforge_ocr_provider_outcome_total"),
            "missing OCR provider outcome metric"
        );
        assert!(
            metrics.contains("billforge_ocr_first_pass_field_confidence"),
            "missing first-pass field confidence metric"
        );
        assert!(metrics.contains("provider=\"tesseract\""));
        assert!(metrics.contains("outcome=\"success\""));
        assert!(metrics.contains("field=\"invoice_number\""));
    }

    #[tokio::test]
    async fn test_serve_metrics_http() {
        use tokio::io::AsyncReadExt;
        // Bind to port 0 to get an ephemeral port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            // Accept one connection, serve metrics
            let (stream, _) = listener.accept().await.unwrap();
            let (mut rx, mut tx) = tokio::io::split(stream);
            let mut buf = [0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut rx, &mut buf).await;
            let body = export_metrics();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            use tokio::io::AsyncWriteExt;
            let _ = tx.write_all(resp.as_bytes()).await;
            let _ = tx.shutdown().await;
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = "GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n";
        use tokio::io::AsyncWriteExt;
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = vec![0u8; 4096];
        let n = stream.read(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response[..n]);
        assert!(response_str.contains("200 OK"));
        assert!(response_str.contains("billforge_ocr_provider_outcome_total"));
    }
}
