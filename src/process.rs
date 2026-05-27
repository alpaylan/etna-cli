use std::{
    io::{BufRead, BufReader, Read as _},
    process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use std::thread::JoinHandle;

#[cfg(unix)]
use std::os::unix::process::CommandExt as _;

#[derive(Debug)]
pub struct CommandOutput {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub timed_out: bool,
}

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINATION_GRACE: Duration = Duration::from_secs(1);

pub fn run_command_tree_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> std::io::Result<CommandOutput> {
    #[cfg(unix)]
    {
        command.process_group(0);
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let child_id = child.id();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let output = stream_output_bytes(stdout, stderr);

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            terminate_process_tree(&mut child, child_id);
            let (stdout, stderr) = output.snapshot();
            let stdout = String::from_utf8_lossy(&stdout)
                .into_owned()
                .lines()
                .map(|s| s.to_owned())
                .collect();
            let stderr = String::from_utf8_lossy(&stderr)
                .into_owned()
                .lines()
                .map(|s| s.to_owned())
                .collect();
            return Ok(CommandOutput {
                status: None,
                stdout,
                stderr,
                timed_out: true,
            });
        }
        thread::sleep(POLL_INTERVAL);
    };

    let (stdout, stderr) = output.join();
    Ok(CommandOutput {
        status: Some(status),
        stdout: String::from_utf8_lossy(&stdout)
            .into_owned()
            .lines()
            .map(|s| s.to_owned())
            .collect(),
        stderr: String::from_utf8_lossy(&stderr)
            .into_owned()
            .lines()
            .map(|s| s.to_owned())
            .collect(),
        timed_out: false,
    })
}

pub fn run_command_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> std::io::Result<CommandOutput> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (out_lines, err_lines) = stream_outputs(stdout, stderr);

    let start = Instant::now();
    let status = loop {
        if start.elapsed() > timeout {
            child.kill().ok();
            break None;
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        thread::sleep(POLL_INTERVAL);
    };

    let stdout = out_lines.join().unwrap_or_default();
    let stderr = err_lines.join().unwrap_or_default();

    Ok(CommandOutput {
        status,
        stdout,
        stderr,
        timed_out: status.is_none(),
    })
}

fn stream_outputs(
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> (JoinHandle<Vec<String>>, JoinHandle<Vec<String>>) {
    let out_handle = thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        let mut lines = Vec::new();
        for line in std::io::BufRead::lines(reader) {
            let line = line.unwrap_or_default();
            println!("[stdout] {}", line);
            lines.push(line);
        }
        lines
    });

    let err_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut lines = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            eprintln!("[stderr] {}", line);
            lines.push(line);
        }
        lines
    });

    (out_handle, err_handle)
}

#[derive(Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

impl StreamKind {
    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

struct StreamedOutput {
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    out_handle: JoinHandle<()>,
    err_handle: JoinHandle<()>,
}

impl StreamedOutput {
    fn join(self) -> (Vec<u8>, Vec<u8>) {
        let stdout = self.stdout.clone();
        let stderr = self.stderr.clone();
        let _ = self.out_handle.join();
        let _ = self.err_handle.join();
        let stdout = stdout.lock().unwrap().clone();
        let stderr = stderr.lock().unwrap().clone();
        (stdout, stderr)
    }

    fn snapshot(&self) -> (Vec<u8>, Vec<u8>) {
        (
            self.stdout.lock().unwrap().clone(),
            self.stderr.lock().unwrap().clone(),
        )
    }
}

fn stream_output_bytes(stdout: ChildStdout, stderr: ChildStderr) -> StreamedOutput {
    let stdout_buffer = Arc::new(Mutex::new(Vec::new()));
    let stderr_buffer = Arc::new(Mutex::new(Vec::new()));

    let out_handle = spawn_output_reader(stdout, stdout_buffer.clone(), StreamKind::Stdout);
    let err_handle = spawn_output_reader(stderr, stderr_buffer.clone(), StreamKind::Stderr);

    StreamedOutput {
        stdout: stdout_buffer,
        stderr: stderr_buffer,
        out_handle,
        err_handle,
    }
}

fn spawn_output_reader<R>(
    output: R,
    buffer: Arc<Mutex<Vec<u8>>>,
    stream: StreamKind,
) -> JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        let mut read_buffer = [0; 8192];
        loop {
            match reader.read(&mut read_buffer) {
                Ok(0) => {
                    if !line.is_empty() {
                        log_output_line(stream, &line);
                    }
                    break;
                }
                Ok(n) => {
                    let chunk = &read_buffer[..n];
                    buffer.lock().unwrap().extend_from_slice(&chunk);
                    log_output_chunk(stream, chunk, &mut line);
                }
                Err(err) => {
                    tracing::warn!("failed to read process {}: {}", stream.label(), err);
                    break;
                }
            }
        }
    })
}

fn log_output_chunk(stream: StreamKind, chunk: &[u8], line: &mut Vec<u8>) {
    for byte in chunk {
        line.push(*byte);
        if *byte == b'\n' {
            log_output_line(stream, line);
            line.clear();
        }
    }
}

fn log_output_line(stream: StreamKind, line: &[u8]) {
    let line = String::from_utf8_lossy(line);
    let line = line.trim_end_matches(['\r', '\n']);
    tracing::debug!("[{}] {}", stream.label(), line);
}

fn terminate_process_tree(child: &mut Child, child_id: u32) {
    #[cfg(unix)]
    {
        terminate_process_tree_with_signal(child_id, Signal::Terminate);

        let start = Instant::now();
        let mut child_exited = false;
        while start.elapsed() < TERMINATION_GRACE {
            if !child_exited {
                match child.try_wait() {
                    Ok(Some(_)) => child_exited = true,
                    Ok(None) => {}
                    Err(_) => child_exited = true,
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        // Always follow the grace period with SIGKILL for the process group.
        // The direct child may have exited after SIGTERM while descendants in
        // the same group kept running.
        terminate_process_tree_with_signal(child_id, Signal::Kill);
        if !child_exited {
            let _ = child.wait();
        }
    }

    #[cfg(not(unix))]
    {
        let _ = child_id;
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum Signal {
    Terminate,
    Kill,
}

#[cfg(unix)]
fn terminate_process_tree_with_signal(child_id: u32, signal: Signal) {
    unsafe {
        let signal = match signal {
            Signal::Terminate => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        let pid = child_id as libc::pid_t;
        if libc::killpg(pid, signal) != 0 {
            let _ = libc::kill(pid, signal);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::run_command_tree_with_timeout;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    fn output_text(lines: &[String]) -> String {
        lines.join("\n")
    }

    fn file_len(path: &Path) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        false
    }

    struct PidGuard {
        pid_file: PathBuf,
    }

    impl Drop for PidGuard {
        fn drop(&mut self) {
            if let Ok(pid) = fs::read_to_string(&self.pid_file)
                .ok()
                .and_then(|s| s.trim().parse::<libc::pid_t>().ok())
                .ok_or(())
            {
                unsafe {
                    let _ = libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }

    #[test]
    fn direct_child_timeout_returns_without_hanging() {
        let start = Instant::now();
        let result =
            run_command_tree_with_timeout(shell("sleep 5"), Duration::from_millis(100)).unwrap();

        assert!(result.timed_out);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn command_tree_timeout_kills_background_child() {
        let tempdir = tempfile::tempdir().unwrap();
        let marker = tempdir.path().join("ticks");

        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                r#"
                while :; do
                    echo tick >> "$MARKER"
                    sleep 0.05
                done &
                wait
                "#,
            )
            .env("MARKER", &marker);

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();
        assert!(result.timed_out);

        let size_after_timeout = fs::metadata(&marker).map(|m| m.len()).unwrap_or(0);
        thread::sleep(Duration::from_millis(250));
        let size_later = fs::metadata(&marker).map(|m| m.len()).unwrap_or(0);

        assert_eq!(size_after_timeout, size_later);
    }

    #[test]
    fn command_tree_timeout_allows_term_cleanup() {
        let tempdir = tempfile::tempdir().unwrap();
        let cleanup = tempdir.path().join("cleanup");

        let mut command = shell(
            r#"
            trap 'echo cleanup >> "$CLEANUP"; exit 0' TERM
            while :; do sleep 0.05; done
            "#,
        );
        command.env("CLEANUP", &cleanup);

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();

        assert!(result.timed_out);
        assert!(fs::read_to_string(cleanup).unwrap().contains("cleanup"));
    }

    #[test]
    fn command_tree_timeout_returns_partial_output() {
        let command = shell("echo stdout-ready; echo stderr-ready >&2; sleep 5");

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();

        assert!(result.timed_out);
        assert!(result
            .stdout
            .iter()
            .any(|line| line.contains("stdout-ready")));
        assert!(result
            .stderr
            .iter()
            .any(|line| line.contains("stderr-ready")));
    }

    #[test]
    fn command_tree_timeout_returns_partial_output_without_trailing_newline() {
        let command = shell("printf partial; sleep 5");

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();

        assert!(result.timed_out);
        assert!(result.stdout.iter().any(|line| line.contains("partial")));
    }

    #[test]
    fn command_tree_timeout_escalates_to_kill_for_child_ignoring_term() {
        let tempdir = tempfile::tempdir().unwrap();
        let marker = tempdir.path().join("ticks");

        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                r#"
                sh -c 'trap "" TERM; while :; do echo tick >> "$MARKER"; sleep 0.05; done' &
                wait
                "#,
            )
            .env("MARKER", &marker);

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();
        assert!(result.timed_out);

        let size_after_timeout = fs::metadata(&marker).map(|m| m.len()).unwrap_or(0);
        thread::sleep(Duration::from_millis(250));
        let size_later = fs::metadata(&marker).map(|m| m.len()).unwrap_or(0);

        assert_eq!(size_after_timeout, size_later);
    }

    #[test]
    fn command_tree_timeout_kills_child_when_wrapper_exits_on_term() {
        let tempdir = tempfile::tempdir().unwrap();
        let marker = tempdir.path().join("ticks");

        let mut command = shell(
            r#"
            sh -c 'trap "" TERM; while :; do echo tick >> "$MARKER"; sleep 0.05; done' &
            trap 'exit 0' TERM
            wait
            "#,
        );
        command.env("MARKER", &marker);

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();
        assert!(result.timed_out);

        let size_after_timeout = file_len(&marker);
        thread::sleep(Duration::from_millis(250));
        let size_later = file_len(&marker);

        assert_eq!(size_after_timeout, size_later);
    }

    #[test]
    fn completed_command_captures_stdout_and_stderr_exactly() {
        let result = run_command_tree_with_timeout(
            shell("printf 'out-a\\nout-b\\n'; printf 'err-a\\nerr-b\\n' >&2"),
            Duration::from_secs(2),
        )
        .unwrap();

        assert!(!result.timed_out);
        assert!(result.status.unwrap().success());
        assert_eq!(result.stdout, vec!["out-a".to_owned(), "out-b".to_owned()]);
        assert_eq!(result.stderr, vec!["err-a".to_owned(), "err-b".to_owned()]);
    }

    #[test]
    fn completed_command_preserves_nonzero_status_and_output() {
        let result = run_command_tree_with_timeout(
            shell("echo before-exit; echo error-before-exit >&2; exit 42"),
            Duration::from_secs(2),
        )
        .unwrap();

        assert!(!result.timed_out);
        assert_eq!(result.status.unwrap().code(), Some(42));
        assert!(output_text(&result.stdout).contains("before-exit"));
        assert!(output_text(&result.stderr).contains("error-before-exit"));
    }

    #[test]
    fn spawn_failure_returns_error() {
        let result = run_command_tree_with_timeout(
            Command::new("__etna_missing_command_for_process_test__"),
            Duration::from_secs(1),
        );

        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn interleaved_stdout_and_stderr_are_captured_independently() {
        let result = run_command_tree_with_timeout(
            shell("echo out-1; echo err-1 >&2; echo out-2; echo err-2 >&2"),
            Duration::from_secs(2),
        )
        .unwrap();

        assert!(!result.timed_out);
        assert_eq!(output_text(&result.stdout), "out-1\nout-2");
        assert_eq!(output_text(&result.stderr), "err-1\nerr-2");
    }

    #[test]
    fn large_output_does_not_deadlock() {
        let result = run_command_tree_with_timeout(
            shell(r#"i=0; while [ "$i" -lt 20000 ]; do echo "line-$i"; i=$((i + 1)); done"#),
            Duration::from_secs(5),
        )
        .unwrap();

        assert!(!result.timed_out);
        assert!(result.status.unwrap().success());
        assert!(result.stdout.len() == 20_000);
        assert!(output_text(&result.stdout).contains("line-19999"));
    }

    #[test]
    fn command_and_grandchild_share_new_process_group() {
        let result = run_command_tree_with_timeout(
            shell(
                r#"
                printf 'parent:%s:%s\n' "$$" "$(ps -o pgid= -p "$$" | tr -d ' ')"
                sh -c 'printf "child:%s:%s\n" "$$" "$(ps -o pgid= -p "$$" | tr -d " " )"'
                "#,
            ),
            Duration::from_secs(2),
        )
        .unwrap();

        assert!(!result.timed_out);
        let mut groups = result.stdout.iter().map(|line| {
            let parts = line.split(':').collect::<Vec<_>>();
            assert_eq!(parts.len(), 3, "unexpected line: {line}");
            (parts[1].to_owned(), parts[2].to_owned())
        });

        let (parent_pid, parent_group) = groups.next().unwrap();
        let (_child_pid, child_group) = groups.next().unwrap();

        assert_eq!(parent_pid, parent_group);
        assert_eq!(parent_group, child_group);
    }

    #[test]
    fn timeout_runtime_is_bounded_by_timeout_plus_grace() {
        let start = Instant::now();
        let result =
            run_command_tree_with_timeout(shell("sleep 5"), Duration::from_millis(200)).unwrap();

        assert!(result.timed_out);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn concurrent_timeouts_are_isolated_by_process_group() {
        let tempdir = tempfile::tempdir().unwrap();
        let markers = (0..3)
            .map(|i| tempdir.path().join(format!("ticks-{i}")))
            .collect::<Vec<_>>();

        thread::scope(|scope| {
            let handles = markers
                .iter()
                .map(|marker| {
                    scope.spawn(move || {
                        let mut command = shell(
                            r#"
                            while :; do
                                echo tick >> "$MARKER"
                                sleep 0.05
                            done &
                            wait
                            "#,
                        );
                        command.env("MARKER", marker);
                        let result =
                            run_command_tree_with_timeout(command, Duration::from_millis(200))
                                .unwrap();
                        assert!(result.timed_out);
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                handle.join().unwrap();
            }
        });

        let sizes_after_timeout = markers.iter().map(|p| file_len(p)).collect::<Vec<_>>();
        thread::sleep(Duration::from_millis(250));
        let sizes_later = markers.iter().map(|p| file_len(p)).collect::<Vec<_>>();

        assert_eq!(sizes_after_timeout, sizes_later);
    }

    #[test]
    fn json_line_before_timeout_is_available_in_partial_output() {
        let result = run_command_tree_with_timeout(
            shell(r#"echo '{"status":"passed","tests":1}'; sleep 5"#),
            Duration::from_millis(200),
        )
        .unwrap();

        assert!(result.timed_out);
        assert!(output_text(&result.stdout).contains(r#"{"status":"passed","tests":1}"#));
    }

    #[test]
    fn detached_child_can_escape_process_group_cleanup() {
        if Command::new("python3")
            .arg("-c")
            .arg("import os")
            .status()
            .is_err()
        {
            return;
        }

        let tempdir = tempfile::tempdir().unwrap();
        let marker = tempdir.path().join("ticks");
        let pid_file = tempdir.path().join("pid");
        let _guard = PidGuard {
            pid_file: pid_file.clone(),
        };

        let mut command = shell(
            r#"
            python3 -c '
import os
import time

os.setsid()
with open(os.environ["PID_FILE"], "w") as f:
    f.write(str(os.getpid()))
    f.flush()
while True:
    with open(os.environ["MARKER"], "a") as f:
        f.write("tick\n")
        f.flush()
    time.sleep(0.05)
' &
            wait
            "#,
        );
        command.env("MARKER", &marker).env("PID_FILE", &pid_file);

        let result = run_command_tree_with_timeout(command, Duration::from_millis(200)).unwrap();
        assert!(result.timed_out);
        assert!(wait_until(Duration::from_secs(1), || pid_file.exists()));

        let size_after_timeout = file_len(&marker);
        thread::sleep(Duration::from_millis(250));
        let size_later = file_len(&marker);

        assert!(size_later > size_after_timeout);
    }
}
