use std::process::{Command, Stdio};

mod support;
use support::{Watcher, holds, wait_until, write};

/// `rt --watch` with no directory watches the working directory, and the
/// edits it hears about are the same ones a named directory would give.
#[test]
fn a_watcher_of_the_working_directory_hears_edits() {
    let home = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test in this binary, so nothing else is
    // reading the environment yet.
    unsafe { std::env::set_var("XDG_CACHE_HOME", home.path()) };

    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    write(root, "src/lib.rs", "pub fn alpha() {}\n");

    let watcher = Watcher(
        Command::new(env!("CARGO_BIN_EXE_rt"))
            .arg("--watch")
            .current_dir(root)
            .env("XDG_CACHE_HOME", home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the watcher"),
    );
    wait_until("the first index", || holds(root, "#alpha"));

    write(root, "src/added.rs", "pub fn zz_watch_probe() {}\n");
    wait_until("a new file to reach the index", || {
        holds(root, "#zz_watch_probe")
    });

    drop(watcher);
}
