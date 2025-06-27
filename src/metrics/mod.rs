use std::sync::LazyLock;

use axum::{Router, routing::get};
use prometheus::{
    Encoder, Histogram, IntGauge, TextEncoder, linear_buckets, register_histogram,
    register_int_gauge,
};

pub static METRIC_TOP_HEADER_HEIGHT: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!("bitlens_top_header_height", "the number of the top header").unwrap()
});

pub static METRIC_FULL_BLOCKS_DOWNLOADED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "bitlens_full_blocks_downloaded",
        "number of blocks we have downloaded"
    )
    .unwrap()
});

pub static METRIC_NET_DIALS_TOTAL: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "bitlens_net_dials_total",
        "total number of tcp dials we've done"
    )
    .unwrap()
});

pub static METRIC_CONNECTIONS_IPV4: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "bitlens_connections_out_ipv4",
        "current outgoing ipv4 connections"
    )
    .unwrap()
});

pub static METRIC_CONNECTIONS_IPV6: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "bitlens_connections_out_ipv6",
        "current outgoing ipv6 connections"
    )
    .unwrap()
});

pub static METRIC_SQLITE_REQUESTS_TIME: LazyLock<Histogram> = LazyLock::new(|| {
    register_histogram!(
        "bitlens_sqlite_requests_time",
        "times of sqlite requests",
        linear_buckets(1.0, 1.0, 100).unwrap()
    )
    .unwrap()
});

// todo: add more metrics

pub async fn start() {
    let app = Router::new().route("/metrics", get(metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8123").await.unwrap();
    tokio::spawn(async { axum::serve(listener, app).await });
}

async fn metrics() -> String {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    String::from_utf8(buffer.clone()).unwrap()
}
