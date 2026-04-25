// File-based debug logger gated by the CCMUX_DEBUG_LOG env var.
// No-op when the env var is unset, so it is safe to leave call sites in code.
// Currently no call sites exist; kept available for future investigations.

#![allow(dead_code)]

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

fn log_handle() -> &'static Option<Mutex<File>> {
    LOG_FILE.get_or_init(|| {
        let path = std::env::var("CCMUX_DEBUG_LOG").ok()?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(Mutex::new)
    })
}

pub fn write(args: std::fmt::Arguments) {
    let Some(mutex) = log_handle() else { return };
    let Ok(mut file) = mutex.lock() else { return };
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file, "[{ts_ms}] {args}");
}

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {
        $crate::debug_log::write(format_args!($($arg)*))
    };
}
