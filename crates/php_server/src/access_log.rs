use hyper::StatusCode;
use std::{fs::OpenOptions, io::Write, sync::Mutex, time::Duration};

#[derive(Debug)]
pub(crate) struct AccessLogger {
    target: AccessLogTarget,
}

#[derive(Debug)]
pub(crate) enum AccessLogTarget {
    Stdout,
    File(Mutex<std::fs::File>),
}

impl AccessLogger {
    pub(crate) fn open(value: &str) -> Result<Self, std::io::Error> {
        let target = if value == "-" {
            AccessLogTarget::Stdout
        } else {
            AccessLogTarget::File(Mutex::new(
                OpenOptions::new().create(true).append(true).open(value)?,
            ))
        };
        Ok(Self { target })
    }

    pub(crate) fn write(&self, entry: &AccessLogEntry<'_>) -> Result<(), std::io::Error> {
        let cache_hit = entry
            .cache_hit
            .map_or("-", |hit| if hit { "hit" } else { "miss" });
        let line = format!(
            "ts={} method={} path=\"{}\" status={} bytes={} duration_ms={} route={} cache={}\n",
            entry.timestamp,
            entry.method,
            escape_log_value(entry.path),
            entry.status.as_u16(),
            entry.bytes,
            entry.duration.as_millis(),
            entry.route,
            cache_hit
        );
        match &self.target {
            AccessLogTarget::Stdout => {
                print!("{line}");
                Ok(())
            }
            AccessLogTarget::File(file) => file
                .lock()
                .expect("access log file mutex poisoned")
                .write_all(line.as_bytes()),
        }
    }
}

pub(crate) fn escape_log_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AccessLogEntry<'a> {
    pub(crate) timestamp: u64,
    pub(crate) method: &'a str,
    pub(crate) path: &'a str,
    pub(crate) status: StatusCode,
    pub(crate) bytes: u64,
    pub(crate) duration: Duration,
    pub(crate) route: &'static str,
    pub(crate) cache_hit: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_escaping_covers_quotes_backslashes_and_newlines() {
        assert_eq!(escape_log_value("a\\b\"c\nd\r"), "a\\\\b\\\"c\\nd\\r");
    }
}
