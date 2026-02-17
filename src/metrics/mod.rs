use axum::{Router, extract::Request, http::StatusCode, routing::get};
use prometheus::{
    Encoder, Histogram, IntGauge, TextEncoder, linear_buckets, register_histogram,
    register_int_gauge,
};
use std::{
    env,
    sync::LazyLock,
};

use crate::packets::packet::{DESERIALIZE_POOL_LARGE, DESERIALIZE_POOL_SMALL};

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

pub static METRIC_ARENA_SPACE_ALLOCATED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "bitlens_arena_space_allocated",
        "currently allocated for all arenas"
    )
    .unwrap()
});

// todo: add more metrics

static METRICS_TOKEN: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let token = env::var("METRICS_TOKEN").expect("METRICS_TOKEN is not set");
    ("Bearer ".to_owned() + &token).into_bytes()
});

static METRICS_LISTEN_ADDRESS: LazyLock<String> = LazyLock::new(|| {
    env::var("METRICS_LISTEN_ADDRESS").expect("METRICS_LISTEN_ADDRESS is not set")
});

pub async fn start() {
    let _ = *METRICS_TOKEN; // Ensure METRICS_TOKEN is set.

    let app = Router::new().route("/metrics", get(metrics));

    let listener = tokio::net::TcpListener::bind(&*METRICS_LISTEN_ADDRESS)
        .await
        .unwrap();
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
    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_large_size current number of large bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_large_size gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_large_size {}\n",
            DESERIALIZE_POOL_LARGE.get_size()
        )
        .as_bytes(),
    );
    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_large_limit current limit of large bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_large_limit gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_large_limit {}\n",
            DESERIALIZE_POOL_LARGE.get_limit()
        )
        .as_bytes(),
    );

    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_small_size current number of small bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_small_size gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_small_size {}\n",
            DESERIALIZE_POOL_SMALL.get_size()
        )
        .as_bytes(),
    );
    buffer.extend_from_slice("# HELP bitlens_deserialize_pool_small_limit current limit of small bump allocators used for deserializing incoming packets\n".as_bytes());
    buffer.extend_from_slice("# TYPE bitlens_deserialize_pool_small_limit gauge\n".as_bytes());
    buffer.extend_from_slice(
        format!(
            "bitlens_deserialize_pool_small_limit {}\n",
            DESERIALIZE_POOL_SMALL.get_limit()
        )
        .as_bytes(),
    );

    Ok(String::from_utf8(buffer.clone()).unwrap())
}
