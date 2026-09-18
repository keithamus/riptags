use std::path::Path;
use std::process::Command;

mod support;
use support::write;

const HARNESS_VARS: &[&str] = &[
    "AGENT",
    "CLAUDECODE",
    "CODEX_SANDBOX",
    "GEMINI_CLI",
    "OMPCODE",
    "OPENCODE",
];

/// `rt` with nothing in the environment that has an opinion about colour.
fn rt(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rt"));
    command
        .env("XDG_CACHE_HOME", home)
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR");
    for key in HARNESS_VARS {
        command.env_remove(key);
    }
    command
}

fn stdout(command: &mut Command) -> String {
    let out = command.output().expect("run rt");
    assert!(
        out.status.success(),
        "rt failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 output")
}

/// One definition, one callsite, and an index of both.
fn tree() -> (tempfile::TempDir, tempfile::TempDir) {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        "src/lib.rs",
        "pub fn alpha() {}\n\npub fn caller() {\n    alpha();\n}\n",
    );
    stdout(rt(home.path()).args(["--index", repo.path().to_str().unwrap()]));
    (home, repo)
}

/// Colour is a terminal's business: a pipe gets none of it, and `--color`
/// overrules whatever the environment looks like.
#[test]
fn colour_waits_to_be_asked() {
    let (home, repo) = tree();
    let root = repo.path().to_str().unwrap();

    let piped = stdout(rt(home.path()).args(["alpha", root]));
    assert!(
        !piped.contains('\u{1b}'),
        "a pipe gets no escapes: {piped:?}"
    );
    assert!(piped.contains("definitions (1)"), "{piped}");

    let forced = stdout(rt(home.path()).args(["alpha", root, "--color=always"]));
    assert!(
        forced.contains("\u{1b}[35msrc/lib.rs\u{1b}[0m"),
        "the path is magenta: {forced:?}"
    );
    assert!(
        forced.contains("\u{1b}[32m1\u{1b}[0m"),
        "the line number is green: {forced:?}"
    );
    assert!(
        forced.contains("\u{1b}[1m\u{1b}[31malpha\u{1b}[0m"),
        "the occurrence is bold red: {forced:?}"
    );
}

/// A coding agent pays for every byte: no escapes, no padding, short words,
/// and JSON on one line.
#[test]
fn an_agent_gets_plain_compact_output() {
    let (home, repo) = tree();
    let root = repo.path().to_str().unwrap();

    let out = stdout(rt(home.path()).env("CLAUDECODE", "1").args(["alpha", root]));
    assert!(!out.contains('\u{1b}'), "an agent gets no escapes: {out:?}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "alpha function",
            "def 1",
            "src/lib.rs:1:8 pub fn alpha() {}",
            "ref 1",
            "src/lib.rs:4:5 [caller] alpha();",
        ],
        "{out}"
    );

    let json = stdout(
        rt(home.path())
            .env("CLAUDECODE", "1")
            .args(["alpha", root, "--json"]),
    );
    assert_eq!(json.lines().count(), 1, "one line of JSON: {json}");
}

/// A pipeline gets records and nothing else: no headings, no prose, and a
/// miss it can only learn from the exit code.
#[test]
fn a_pipeline_gets_one_record_per_site() {
    let (home, repo) = tree();
    let root = repo.path().to_str().unwrap();

    let sites = stdout(rt(home.path()).args(["alpha", root, "--vimgrep"]));
    assert_eq!(
        sites.lines().collect::<Vec<_>>(),
        [
            "src/lib.rs:1:8:pub fn alpha() {}",
            "src/lib.rs:4:5:alpha();"
        ],
        "{sites}"
    );

    let defs = stdout(rt(home.path()).args(["--function", root, "--vimgrep"]));
    assert_eq!(
        defs.lines().collect::<Vec<_>>(),
        [
            "src/lib.rs:1:8:pub fn alpha() {}",
            "src/lib.rs:3:8:pub fn caller() {"
        ],
        "a listing points at each definition: {defs}"
    );

    let files = stdout(rt(home.path()).args(["alpha", root, "-l"]));
    assert_eq!(files, "src/lib.rs\n", "one path per file, once");

    let nul = stdout(rt(home.path()).args(["alpha", root, "-l", "-0"]));
    assert_eq!(nul, "src/lib.rs\0", "NUL records for xargs -0");

    let miss = rt(home.path())
        .args(["nope", root, "--vimgrep"])
        .output()
        .expect("run rt");
    assert_eq!(miss.status.code(), Some(1));
    assert!(miss.stdout.is_empty(), "{:?}", miss.stdout);
}

/// `--limit 0` lifts the cap: a pipeline that edits every callsite cannot
/// stop at 50 of them.
#[test]
fn limit_zero_asks_for_every_site() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        "src/lib.rs",
        "pub fn alpha() {}\nfn a() { alpha(); }\nfn b() { alpha(); }\nfn c() { alpha(); }\n",
    );
    let root = repo.path().to_str().unwrap();
    stdout(rt(home.path()).args(["--index", root]));

    let capped = stdout(rt(home.path()).args(["alpha", root, "--refs", "-l", "--limit", "2"]));
    assert_eq!(capped.lines().count(), 1, "paths collapse: {capped}");

    let capped = stdout(rt(home.path()).args(["alpha", root, "--refs", "--vimgrep", "--limit=2"]));
    assert_eq!(capped.lines().count(), 2, "{capped}");

    let every = stdout(rt(home.path()).args(["alpha", root, "--refs", "--vimgrep", "--limit=0"]));
    assert_eq!(every.lines().count(), 3, "{every}");
}

/// `rt` with nothing to look for describes the tree instead of complaining,
/// and a positional only names a tree when it is written as a path: a symbol
/// may well be called `src`.
#[test]
fn a_bare_rt_describes_the_tree() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        "src/lib.rs",
        "pub fn alpha() {}\n\npub fn caller() {\n    alpha();\n}\n",
    );
    write(repo.path(), "app/main.py", "def beta():\n    pass\n");
    stdout(rt(home.path()).args(["--index", repo.path().to_str().unwrap()]));

    let out = stdout(rt(home.path()).current_dir(repo.path()));
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].contains("2 files, ") && lines[0].ends_with(" occurrences"),
        "{out}"
    );
    assert_eq!(lines[1], "languages  python 1  rust 1", "{out}");
    assert!(lines[2].starts_with("kinds      function "), "{out}");

    let miss = rt(home.path())
        .current_dir(repo.path())
        .arg("src")
        .output()
        .expect("run rt");
    assert_eq!(miss.status.code(), Some(1), "`src` is a symbol: {miss:?}");

    let scoped = stdout(rt(home.path()).current_dir(repo.path()).arg("./src"));
    assert!(scoped.contains("1 files, "), "one rust file: {scoped}");
}

/// `--type` keeps one part of a mixed tree: its sites, its overview, and
/// a miss that says which language it looked in. An unknown name is a usage
/// error, not an empty answer.
#[test]
fn a_language_narrows_a_mixed_tree() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        "src/lib.rs",
        "pub fn alpha() {}\n\npub fn caller() {\n    alpha();\n}\n",
    );
    write(repo.path(), "app/main.py", "def alpha():\n    pass\n");
    stdout(rt(home.path()).args(["--index", repo.path().to_str().unwrap()]));

    let python = stdout(
        rt(home.path())
            .current_dir(repo.path())
            .args(["alpha", "--type", "py"]),
    );
    assert!(python.contains("app/main.py"), "{python}");
    assert!(!python.contains("src/lib.rs"), "{python}");

    let overview = stdout(
        rt(home.path())
            .current_dir(repo.path())
            .args(["-t", "rust"]),
    );
    assert!(overview.contains("1 files, "), "{overview}");
    assert!(overview.contains("languages  rust 1"), "{overview}");

    let miss = rt(home.path())
        .current_dir(repo.path())
        .args(["alpha", "-t", "go"])
        .output()
        .expect("run rt");
    assert_eq!(miss.status.code(), Some(1), "nothing in Go: {miss:?}");
    assert!(
        String::from_utf8_lossy(&miss.stdout).contains("in go"),
        "the miss says where it looked: {miss:?}"
    );

    let unknown = rt(home.path())
        .current_dir(repo.path())
        .args(["alpha", "-t", "cobol"])
        .output()
        .expect("run rt");
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("unknown language `cobol`"),
        "{unknown:?}"
    );
}
