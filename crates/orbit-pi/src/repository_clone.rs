//! Blocking Git clone operation for the working-folder flow.
//!
//! Run this on a background executor. Progress is emitted as Git writes to
//! stderr; cancellation is cooperative and must remain available until the
//! operation returns. A failed or cancelled clone is never ready to open.

use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const PROGRESS_CHUNK_BYTES: usize = 4096;
const DIAGNOSTIC_TAIL_BYTES: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum CloneError {
    Cancelled,
    Git(String),
}

/// Clone `source` into `destination`, forwarding Git's progress to `progress`.
///
/// The caller owns the destination and decides how to handle any partial
/// directory left by Git on failure or cancellation; this function never
/// deletes a directory that might contain the user's files.
pub fn clone_repository(
    source: &str,
    destination: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(&str),
) -> Result<(), CloneError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(CloneError::Cancelled);
    }
    let mut command = Command::new("git");
    command
        .args(["clone", "--progress", "--"])
        .arg(source)
        .arg(destination)
        .env("GIT_TERMINAL_PROMPT", "0");
    run_clone(command, cancelled, &mut progress)
}

fn run_clone(
    mut command: Command,
    cancelled: &AtomicBool,
    progress: &mut impl FnMut(&str),
) -> Result<(), CloneError> {
    // Git may spawn SSH, credential helpers, and remote transports. Give them
    // a separate group so cancellation can stop the whole operation on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = spawn_clone(command).map_err(|error| CloneError::Git(error.to_string()))?;
    let stderr = clone_stderr(&mut child).expect("stderr was piped");
    // Backpressure also bounds queued output if Git writes faster than the UI
    // can consume it. Dropping the receiver unblocks the reader on cancel.
    let (sender, receiver) = mpsc::sync_channel(16);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        for byte in BufReader::new(stderr).bytes() {
            let Ok(byte) = byte else { break };
            bytes.push(byte);
            if byte == b'\r' || byte == b'\n' || bytes.len() == PROGRESS_CHUNK_BYTES {
                if sender
                    .send(String::from_utf8_lossy(&bytes).into_owned())
                    .is_err()
                {
                    return;
                }
                bytes.clear();
            }
        }
        if !bytes.is_empty() {
            let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
        }
    });

    let mut output = String::new();
    let mut stderr_closed = false;
    loop {
        if cancelled.load(Ordering::Acquire) {
            stop_clone(&mut child);
            return Err(CloneError::Cancelled);
        }
        if stderr_closed {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(CloneError::Git(output.trim().to_owned()))
                    };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(error) => {
                    stop_clone(&mut child);
                    return Err(CloneError::Git(error.to_string()));
                }
            }
        } else {
            // Do not reap the group leader until its stderr closes: a
            // transport can inherit the pipe, and the leader's PID must not
            // be reused while cancellation can still signal its group.
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(chunk) => {
                    progress(&chunk);
                    output.push_str(&chunk);
                    if output.len() > DIAGNOSTIC_TAIL_BYTES {
                        let mut start = output.len() - DIAGNOSTIC_TAIL_BYTES;
                        while !output.is_char_boundary(start) {
                            start += 1;
                        }
                        output.drain(..start);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => stderr_closed = true,
            }
        }
    }
}

#[cfg(unix)]
type CloneChild = std::process::Child;
#[cfg(windows)]
type CloneChild = Box<dyn process_wrap::std::ChildWrapper>;

#[cfg(unix)]
fn spawn_clone(mut command: Command) -> std::io::Result<CloneChild> {
    command.spawn()
}

#[cfg(windows)]
fn spawn_clone(command: Command) -> std::io::Result<CloneChild> {
    // The wrapper creates Git suspended, assigns it to a job, then resumes
    // it. This avoids the race where Git starts a transport before assignment.
    let mut command = process_wrap::std::CommandWrap::from(command);
    command.wrap(process_wrap::std::JobObject);
    command.spawn()
}

#[cfg(unix)]
fn clone_stderr(child: &mut CloneChild) -> Option<std::process::ChildStderr> {
    child.stderr.take()
}

#[cfg(windows)]
fn clone_stderr(child: &mut CloneChild) -> Option<std::process::ChildStderr> {
    child.stderr().take()
}

fn stop_clone(child: &mut CloneChild) {
    #[cfg(unix)]
    {
        // SAFETY: the child was started in its own process group, whose ID is
        // its PID. A negative PID targets only that group, not Orbit's group.
        unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
    }
    // On Windows `kill` terminates the job; on Unix it is a fallback if the
    // process group signal failed.
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "orbit-clone-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn git(&self, args: &[&str]) {
            let result = Command::new("git")
                .current_dir(&self.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }

        fn bare_repository(&self) -> PathBuf {
            self.git(&["init", "source"]);
            self.git(&["-C", "source", "config", "user.name", "Test"]);
            self.git(&["-C", "source", "config", "user.email", "test@example.com"]);
            fs::write(self.0.join("source/file.txt"), "content").unwrap();
            self.git(&["-C", "source", "add", "file.txt"]);
            self.git(&["-C", "source", "commit", "-m", "initial"]);
            self.git(&["clone", "--bare", "source", "remote.git"]);
            self.0.join("remote.git")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn clones_a_local_bare_repository_and_reports_progress() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("checkout");
        let mut progress = String::new();
        clone_repository(
            remote.to_str().unwrap(),
            &destination,
            &AtomicBool::new(false),
            |chunk| progress.push_str(chunk),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("file.txt")).unwrap(),
            "content"
        );
        assert!(destination.join(".git").is_dir());
        assert!(!progress.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn clones_a_local_bare_repository_from_a_file_url() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("url-checkout");
        let source = format!("file://{}", remote.display());
        clone_repository(&source, &destination, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("file.txt")).unwrap(),
            "content"
        );
    }

    #[test]
    fn clones_into_an_existing_empty_directory() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("empty");
        fs::create_dir(&destination).unwrap();
        clone_repository(
            remote.to_str().unwrap(),
            &destination,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("file.txt")).unwrap(),
            "content"
        );
    }

    #[test]
    fn missing_repository_reports_git_failure() {
        let fixture = Fixture::new();
        let source = fixture.0.join("missing.git");
        let destination = fixture.0.join("checkout");
        let result = clone_repository(
            source.to_str().unwrap(),
            &destination,
            &AtomicBool::new(false),
            |_| {},
        );
        match result {
            Err(CloneError::Git(message)) => assert!(!message.is_empty()),
            other => panic!("expected Git error, got {other:?}"),
        }
        assert!(!destination.join(".git").exists());
    }

    #[test]
    fn failure_does_not_overwrite_existing_files() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("existing");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep"), "important").unwrap();
        let result = clone_repository(
            remote.to_str().unwrap(),
            &destination,
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(matches!(result, Err(CloneError::Git(_))));
        assert_eq!(
            fs::read_to_string(destination.join("keep")).unwrap(),
            "important"
        );
    }

    #[test]
    fn cancellation_after_progress_does_not_report_success() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("cancelled-during-clone");
        let cancelled = AtomicBool::new(false);
        let result = clone_repository(remote.to_str().unwrap(), &destination, &cancelled, |_| {
            cancelled.store(true, Ordering::Release);
        });
        assert_eq!(result, Err(CloneError::Cancelled));
    }

    #[cfg(unix)]
    #[test]
    fn long_unterminated_stderr_is_chunked_and_keeps_a_bounded_tail() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("head -c 1048576 /dev/zero | tr '\\0' x >&2; exit 1");
        let mut largest_chunk = 0;
        let result = run_clone(command, &AtomicBool::new(false), &mut |chunk| {
            largest_chunk = largest_chunk.max(chunk.len());
        });
        let Err(CloneError::Git(output)) = result else {
            panic!("expected Git error")
        };
        assert_eq!(largest_chunk, PROGRESS_CHUNK_BYTES);
        assert_eq!(output.len(), DIAGNOSTIC_TAIL_BYTES);
        assert!(output.bytes().all(|byte| byte == b'x'));
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_stops_transport_children() {
        let fixture = Fixture::new();
        let marker = fixture.0.join("child-survived");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("(sleep 2; touch \"$1\") & echo ready >&2; wait")
            .arg("clone-test")
            .arg(&marker);
        let cancelled = AtomicBool::new(false);
        let result = run_clone(command, &cancelled, &mut |_| {
            cancelled.store(true, Ordering::Release);
        });
        assert_eq!(result, Err(CloneError::Cancelled));
        std::thread::sleep(Duration::from_secs(3));
        assert!(!marker.exists(), "transport child survived cancellation");
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_stops_transport_after_parent_exits() {
        let fixture = Fixture::new();
        let marker = fixture.0.join("orphan-survived");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("(sleep 2; touch \"$1\") & echo ready >&2; exit")
            .arg("clone-test")
            .arg(&marker);
        let cancelled = AtomicBool::new(false);
        let result = run_clone(command, &cancelled, &mut |_| {
            std::thread::sleep(Duration::from_millis(200));
            cancelled.store(true, Ordering::Release);
        });
        assert_eq!(result, Err(CloneError::Cancelled));
        std::thread::sleep(Duration::from_secs(3));
        assert!(!marker.exists(), "orphaned transport survived cancellation");
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_stops_windows_transport_after_parent_exits() {
        let fixture = Fixture::new();
        let marker = fixture.0.join("child-survived");
        let child_script = fixture.0.join("child.cmd");
        let parent_script = fixture.0.join("parent.cmd");
        fs::write(
            &child_script,
            format!(
                "@echo off\r\nping -n 4 127.0.0.1 >nul\r\necho survived>\"{}\"\r\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::write(
            &parent_script,
            format!(
                "@echo off\r\nstart \"\" /b cmd /d /c call \"{}\"\r\necho ready 1>&2\r\nexit /b\r\n",
                child_script.display()
            ),
        )
        .unwrap();
        let mut command = Command::new("cmd");
        command.args(["/d", "/c"]).arg(&parent_script);
        let cancelled = AtomicBool::new(false);
        let result = run_clone(command, &cancelled, &mut |_| {
            std::thread::sleep(Duration::from_millis(300));
            cancelled.store(true, Ordering::Release);
        });
        assert_eq!(result, Err(CloneError::Cancelled));
        std::thread::sleep(Duration::from_secs(5));
        assert!(!marker.exists(), "Windows transport survived cancellation");
    }

    #[test]
    fn cancellation_before_start_does_not_create_destination() {
        let fixture = Fixture::new();
        let remote = fixture.bare_repository();
        let destination = fixture.0.join("cancelled");
        let result = clone_repository(
            remote.to_str().unwrap(),
            &destination,
            &AtomicBool::new(true),
            |_| {},
        );
        assert_eq!(result, Err(CloneError::Cancelled));
        assert!(!destination.exists());
    }
}
