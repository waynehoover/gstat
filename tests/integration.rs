use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn gstat_binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("git-status-watch");
    path
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init"]);
    git(dir, &["config", "user.email", "test@test.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "initial"]);
}

// --- --once mode tests ---

#[test]
fn once_json_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let output = Command::new(gstat_binary())
        .args(["--once"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run gstat");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["branch"], "master");
    assert_eq!(parsed["staged"], 0);
    assert_eq!(parsed["modified"], 0);
    assert_eq!(parsed["untracked"], 0);
    assert_eq!(parsed["state"], "clean");
}

#[test]
fn once_custom_format() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join("new.txt"), "new").unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once", "--format", "{branch} ?{untracked}"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run gstat");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "master ?1");
}

#[test]
fn once_staged_and_modified() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join("staged.txt"), "staged").unwrap();
    git(tmp.path(), &["add", "staged.txt"]);
    std::fs::write(tmp.path().join("file.txt"), "modified").unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once", "--format", "+{staged} ~{modified}"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run gstat");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "+1 ~1");
}

#[test]
fn once_detects_untracked_files() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
    std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once", "--format", "?{untracked}"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "?3");
}

#[test]
fn once_not_a_repo() {
    let tmp = tempfile::tempdir().unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a git repository"));
}

#[test]
fn once_path_argument() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join("x.txt"), "x").unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once", "--format", "?{untracked}"])
        .arg(tmp.path())
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "?1");
}

#[test]
fn once_merge_state() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join(".git/MERGE_HEAD"), "abc123\n").unwrap();

    let output = Command::new(gstat_binary())
        .args(["--once", "--format", "{state}"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "merge");
}

// --- watch mode tests ---

#[test]
fn watch_detects_new_file() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let mut child = Command::new(gstat_binary())
        .args(["--format", "?{untracked}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut initial = String::new();
    reader.read_line(&mut initial).unwrap();
    assert_eq!(initial.trim(), "?0", "initial status should show 0 untracked");

    // Wait for watcher to fully initialize, then create a file
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(tmp.path().join("newfile.txt"), "hello").unwrap();

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = tx.send(line);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(line) => {
            assert_eq!(line.trim(), "?1", "should detect new untracked file");
        }
        Err(_) => {
            child.kill().unwrap();
            panic!("timed out waiting for gstat to detect new file");
        }
    }

    child.kill().unwrap();
    let _ = child.wait();
}

#[test]
fn watch_detects_git_add() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    std::fs::write(tmp.path().join("newfile.txt"), "hello").unwrap();

    let mut child = Command::new(gstat_binary())
        .args(["--format", "+{staged} ?{untracked}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut initial = String::new();
    reader.read_line(&mut initial).unwrap();
    assert_eq!(initial.trim(), "+0 ?1", "initial: 1 untracked");

    std::thread::sleep(Duration::from_millis(500));
    git(tmp.path(), &["add", "newfile.txt"]);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = tx.send(line);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(line) => {
            assert_eq!(line.trim(), "+1 ?0", "should detect staged file");
        }
        Err(_) => {
            child.kill().unwrap();
            panic!("timed out waiting for gstat to detect git add");
        }
    }

    child.kill().unwrap();
    let _ = child.wait();
}

#[test]
fn watch_detects_file_modification() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let mut child = Command::new(gstat_binary())
        .args(["--format", "~{modified}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut initial = String::new();
    reader.read_line(&mut initial).unwrap();
    assert_eq!(initial.trim(), "~0");

    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(tmp.path().join("file.txt"), "changed content").unwrap();

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = tx.send(line);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(line) => {
            assert_eq!(line.trim(), "~1", "should detect modified tracked file");
        }
        Err(_) => {
            child.kill().unwrap();
            panic!("timed out waiting for gstat to detect file modification");
        }
    }

    child.kill().unwrap();
    let _ = child.wait();
}

#[test]
fn watch_dedup_suppresses_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let mut child = Command::new(gstat_binary())
        .args(["--format", "?{untracked}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut initial = String::new();
    reader.read_line(&mut initial).unwrap();
    assert_eq!(initial.trim(), "?0");

    // Create file -> should get exactly one ?1
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.trim().to_string()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let first = rx.recv_timeout(Duration::from_secs(5)).expect("should get ?1");
    assert_eq!(first, "?1");

    // Wait — feedback loop would produce more ?1 lines
    std::thread::sleep(Duration::from_secs(2));

    let mut extra_lines = Vec::new();
    while let Ok(line) = rx.try_recv() {
        extra_lines.push(line);
    }

    child.kill().unwrap();
    let _ = child.wait();

    assert!(
        extra_lines.is_empty(),
        "feedback loop detected: got {} extra lines: {:?}",
        extra_lines.len(),
        extra_lines
    );
}

#[test]
fn watch_multiple_sequential_changes() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    let mut child = Command::new(gstat_binary())
        .args(["--format", "?{untracked}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "?0");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(buf.trim().to_string()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Change 1: add a file
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
    let out1 = rx.recv_timeout(Duration::from_secs(5)).expect("should detect first file");
    assert_eq!(out1, "?1");

    // Change 2: add another file
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
    let out2 = rx.recv_timeout(Duration::from_secs(5)).expect("should detect second file");
    assert_eq!(out2, "?2");

    // Change 3: remove a file
    std::thread::sleep(Duration::from_millis(500));
    std::fs::remove_file(tmp.path().join("a.txt")).unwrap();
    let out3 = rx.recv_timeout(Duration::from_secs(5)).expect("should detect file removal");
    assert_eq!(out3, "?1");

    child.kill().unwrap();
    let _ = child.wait();
}

#[test]
fn watch_detects_commit() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // Start with a staged file
    std::fs::write(tmp.path().join("staged.txt"), "staged").unwrap();
    git(tmp.path(), &["add", "staged.txt"]);

    let mut child = Command::new(gstat_binary())
        .args(["--format", "+{staged} ?{untracked}"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gstat");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut initial = String::new();
    reader.read_line(&mut initial).unwrap();
    assert_eq!(initial.trim(), "+1 ?0");

    // Commit the staged file
    std::thread::sleep(Duration::from_millis(500));
    git(tmp.path(), &["commit", "-m", "add staged file"]);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = tx.send(line);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(line) => {
            assert_eq!(line.trim(), "+0 ?0", "should detect commit cleared staged");
        }
        Err(_) => {
            child.kill().unwrap();
            panic!("timed out waiting for gstat to detect commit");
        }
    }

    child.kill().unwrap();
    let _ = child.wait();
}
