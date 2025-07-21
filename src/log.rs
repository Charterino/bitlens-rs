use std::path::Path;

use file_rotate::{
    ContentLimit, FileRotate, TimeFrequency,
    compression::Compression,
    suffix::{AppendTimestamp, FileLimit},
};
use slog::{Drain, Duplicate, FnValue, Logger, PushFnValue, Record, o};
use slog_async::Async;
use slog_json::Json;
use slog_scope::GlobalLoggerGuard;
use slog_term::{CompactFormat, TermDecorator};
use time::OffsetDateTime;

pub fn setup_logging() -> GlobalLoggerGuard {
    let file = FileRotate::new(
        Path::new("logs/bitlens.log"),
        AppendTimestamp::default(FileLimit::MaxFiles(30)),
        ContentLimit::Time(TimeFrequency::Daily),
        Compression::None,
        None,
    );
    let file_drain = Json::new(file)
        .add_key_value(o!(
            "ts" => FnValue(move |_ : &Record| {
                (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).to_string()
            }),
            "level" => FnValue(move |rinfo : &Record| {
                rinfo.level().as_short_str()
            }),
            "msg" => PushFnValue(move |record : &Record, ser| {
                ser.emit(record.msg())
            }),
        ))
        .build()
        .fuse();
    let stdout_drain = CompactFormat::new(TermDecorator::new().force_color().stdout().build())
        .build()
        .fuse();
    let drain = Async::new(Duplicate::new(file_drain, stdout_drain).fuse())
        .overflow_strategy(slog_async::OverflowStrategy::Block)
        .build()
        .fuse();
    let logger = Logger::root(drain, o!());
    slog_scope::set_global_logger(logger)
}
