use chrono::Local;
use std::io::{self, Write};
use std::sync::{Mutex, OnceLock};

static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_log_lock<F: FnOnce()>(f: F) {
    let lock = LOG_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().expect("log mutex poisoned");
    f();
}

fn timestamp() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

fn colorize(code: &str, text: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, text)
}

fn write_stdout_line(line: &str) {
    with_log_lock(|| {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{}", line);
    });
}

fn write_stderr_line(line: &str) {
    with_log_lock(|| {
        let mut out = io::stderr().lock();
        let _ = writeln!(out, "{}", line);
    });
}

fn write_stdout_block(message: &str) {
    with_log_lock(|| {
        let mut out = io::stdout().lock();
        let _ = write!(out, "{}", message);
        if !message.ends_with('\n') {
            let _ = writeln!(out);
        }
    });
}

fn write_stderr_block(message: &str) {
    with_log_lock(|| {
        let mut out = io::stderr().lock();
        let _ = write!(out, "{}", message);
        if !message.ends_with('\n') {
            let _ = writeln!(out);
        }
    });
}

fn format_line(level: &str, color: &str, label: &str, message: &str, icon: &str) -> String {
    let timestamp = timestamp();
    let symbol = colorize(color, icon);
    format!("[{timestamp}] {symbol} {level:<5} [{label}] {message}")
}

pub fn log_info(label: &str, message: &str) {
    let line = format_line("INFO", "34", label, message, "ℹ");
    write_stdout_line(&line);
}

pub fn log_success(label: &str, message: &str) {
    let line = format_line("OK", "32", label, message, "✓");
    write_stdout_line(&line);
}

pub fn log_warn(label: &str, message: &str) {
    let line = format_line("WARN", "33", label, message, "⚠");
    write_stderr_line(&line);
}

pub fn log_error(label: &str, message: &str) {
    let line = format_line("ERROR", "31", label, message, "✗");
    write_stderr_line(&line);
}

pub fn log_raw_stdout(message: &str) {
    write_stdout_block(message);
}

pub fn log_raw_stderr(message: &str) {
    write_stderr_block(message);
}
