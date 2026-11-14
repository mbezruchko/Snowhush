use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// Past this many bytes the log starts over, keeping what it had as the old log.
const LIMIT: u64 = 4 << 20;

struct Log {
    file: File,
    path: PathBuf,
    started: Instant,
    written: u64,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

pub fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state")))
        .map(|base| base.join("snowhush").join("snowhush.log"))
}

/// Starts the log afresh, keeping the last one as `snowhush.old.log`.
fn open(path: &Path) -> Option<File> {
    let _ = std::fs::rename(path, path.with_file_name("snowhush.old.log"));
    File::create(path).ok()
}

pub fn start() {
    let Some(path) = path() else { return };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Some(file) = open(&path)
        && let Ok(mut log) = LOG.lock()
    {
        *log = Some(Log { file, path, started: Instant::now(), written: 0 });
    }
}

pub fn line(arguments: std::fmt::Arguments) {
    // Formatted before locking: a panic while formatting then reaches the panic hook's own line instead of a held lock.
    let text = arguments.to_string();
    if let Ok(mut log) = LOG.lock()
        && let Some(log) = log.as_mut()
    {
        if log.written > LIMIT
            && let Some(file) = open(&log.path)
        {
            log.file = file;
            log.written = 0;
        }
        let line = format!("{:>9.2}  {text}\n", log.started.elapsed().as_secs_f64());
        if log.file.write_all(line.as_bytes()).is_ok() {
            log.written += line.len() as u64;
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($argument:tt)*) => { $crate::log::line(format_args!($($argument)*)) };
}
