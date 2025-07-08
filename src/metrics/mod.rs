use crate::packets::packet::{CURRENT_POOL_LIMIT, CURRENT_POOL_SIZE};
use axum::{Router, extract::Request, http::StatusCode, routing::get};
use prometheus::{
    Encoder, Histogram, IntGauge, TextEncoder, linear_buckets, register_histogram,
    register_int_gauge,
};
use std::{
    env,
    sync::{LazyLock, atomic::Ordering},
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

static METRICS_TOKEN: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let token = env::var("METRICS_TOKEN").expect("METRICS_TOKEN is not set");
    ("Bearer ".to_owned() + &token).into_bytes()
});

pub async fn start() {
    let _ = *METRICS_TOKEN; // Ensure METRICS_TOKEN is set.

    let app = Router::new().route("/metrics", get(metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8123").await.unwrap();
    tokio::spawn(async { axum::serve(listener, app).await });
}

async fn metrics(req: Request) -> Result<String, StatusCode> {
    let headers = req.headers();
    match headers.get("Authorization") {
        Some(token) => {
            if token.as_bytes() != *METRICS_TOKEN {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
        None => return Err(StatusCode::UNAUTHORIZED),
    }
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_size current number of bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_size gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_size {}\n",
            CURRENT_POOL_SIZE.load(Ordering::Relaxed)
        )
        .as_bytes(),
    );
    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_limit current limit of bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_limit gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_limit {}\n",
            CURRENT_POOL_LIMIT.load(Ordering::Relaxed)
        )
        .as_bytes(),
    );

    Ok(String::from_utf8(buffer.clone()).unwrap())
}
