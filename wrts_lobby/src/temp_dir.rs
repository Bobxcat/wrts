use std::{
    io::{self, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{LazyLock, Mutex, OnceLock},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    task::JoinHandle,
};
use tracing::error;

fn ctrlc_handler() {
    let exit_code = 2;

    static NUM_CALLS: Mutex<u32> = Mutex::new(0);
    {
        let should_exit: bool;
        match NUM_CALLS.lock() {
            Ok(mut num_calls) => {
                *num_calls += 1;
                should_exit = *num_calls > 5;
            }
            Err(_) => should_exit = true,
        }

        if should_exit {
            std::process::exit(exit_code)
        }
    }

    temp_dir_cleanup();
    let _ = io::stdout().flush();
    std::process::exit(exit_code)
}

static TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();
static CLEANED: Mutex<bool> = Mutex::new(false);

/// Create once in the main function
pub struct TempDirBuilder {
    _must_call_build: PhantomData<()>,
}

impl TempDirBuilder {
    pub fn build() -> Self {
        if TEMP_DIR.get().is_some() {
            panic!("TmpDirHandle::create() called a second time!")
        }
        let _ = TEMP_DIR.get_or_init(make_temp_dir);

        ctrlc::set_handler(ctrlc_handler).unwrap();

        Self {
            _must_call_build: PhantomData,
        }
    }
}

impl Drop for TempDirBuilder {
    fn drop(&mut self) {
        temp_dir_cleanup();
    }
}

fn temp_dir_cleanup() {
    let mut cleaned = CLEANED.lock().unwrap();
    if *cleaned {
        return;
    }
    let Some(tmp_dir) = TEMP_DIR.get() else {
        return;
    };
    if let Err(err) = std::fs::remove_dir_all(tmp_dir) {
        if err.kind() == io::ErrorKind::NotFound {
        } else {
            error!("temp_dir_cleanup error: `{err}`")
        }
    }
    *cleaned = true;
}

pub fn temp_dir() -> &'static Path {
    if *CLEANED.lock().unwrap() {
        panic!("Temporary directory has already been cleaned!");
    }
    &TEMP_DIR
        .get()
        .expect("TmpDirHandle::create() hasn't been called!")
}

fn make_temp_dir() -> PathBuf {
    let root = std::env::temp_dir();
    for _ in 0..100 {
        let radix = 16;
        let ext_random_part = String::from_iter(
            std::iter::repeat_with(|| rand::random_range(0..radix))
                .map(|num| char::from_digit(num, radix).unwrap())
                .take(6),
        );
        let path = root.join(format!("wrts_lobby_{ext_random_part}"));
        match std::fs::create_dir(&path) {
            Ok(_) => return path,
            _ => (),
        }
    }

    panic!(
        "make_temp_dir failed!! This likely means lacking permissions to the OS temporary directory"
    );
}

pub fn wrts_match_exe() -> &'static Path {
    static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        let data = include_bytes!("../build_assets/wrts_match.exe");
        let path = temp_dir().join("wrts_match.exe");
        let mut f = std::fs::File::create_new(&path).unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
        path
    });

    &PATH
}

pub struct WrtsMatchProcess {
    pub process: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    log_path: String,
}

impl WrtsMatchProcess {
    pub async fn spawn() -> anyhow::Result<Self> {
        let log_path = format!("wrts_log_{:x}.txt", rand::random_range(0..(1024 * 1024)));

        let mut process = Command::new(wrts_match_exe())
            // Disable coloring in bevy logs, since they are written to a `.txt` file
            .env("NO_COLOR", "1")
            // Enable verbose backtraces
            .env("BEVY_BACKTRACE", "full")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log_create(&log_path).unwrap())
            .spawn()?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        // stdout.read
        Ok(Self {
            process,
            stdin,
            stdout,
            log_path,
        })
    }

    pub fn log_path(&self) -> &str {
        &self.log_path
    }
}

pub fn log_dir() -> &'static Path {
    static LOG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let path = "logs";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();
        path.into()
    });
    &LOG_DIR
}

pub fn log_create(path: impl AsRef<Path>) -> io::Result<std::fs::File> {
    std::fs::File::create_new(log_dir().join(path))
}
