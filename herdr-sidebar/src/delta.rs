//! Read-only committed changes against a workspace repository's configured default branch.
//! The baseline is the merge-base of HEAD and the locally cached remote-tracking ref;
//! no fetch, checkout, or index refresh is performed here.

use std::path::Path;
use std::process::{Command, Output};

#[derive(Clone, Debug)]
pub struct Delta {
    pub target_ref: String,
    pub target_oid: String,
    pub head_oid: String,
    pub base_oid: String,
    pub committed: Vec<ChangedFile>,
    pub issue: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub previous: Option<String>,
    pub status: char,
}

impl Delta {
    fn empty() -> Self {
        Self {
            target_ref: String::new(),
            target_oid: String::new(),
            head_oid: String::new(),
            base_oid: String::new(),
            committed: Vec::new(),
            issue: None,
        }
    }
}

fn git(root: &Path, args: &[&str]) -> Result<Output, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|e| format!("git {}: {e}", args.first().copied().unwrap_or("")))?;
    if !output.status.success() {
        if args.first() == Some(&"merge-base")
            && output.status.code() == Some(1)
            && output.stdout.is_empty()
            && output.stderr.is_empty()
        {
            return Err("HEAD and the default branch have no common ancestor".into());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            if stderr.trim().is_empty() {
                output.status.to_string()
            } else {
                stderr.trim().to_string()
            }
        ));
    }
    Ok(output)
}

fn text(root: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(git(root, args)?.stdout)
        .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
        .map_err(|e| format!("git {} returned non-UTF-8 text: {e}", args[0]))
}

/// Accept only complete object IDs, never a revision expression from a control file.
fn object_id(value: &str) -> Result<&str, String> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err("git returned an invalid object ID".into())
    }
}

fn parse_names(raw: &[u8]) -> Result<Vec<ChangedFile>, String> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let Some(b'\0') = raw.last().copied() else {
        return Err("git diff returned an unterminated name-status record".into());
    };
    let mut fields = raw[..raw.len() - 1].split(|&b| b == 0);
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let (Some(&code), Some(path)) = (status.first(), fields.next()) else {
            return Err("git diff returned an incomplete name-status record".into());
        };
        if !matches!(code, b'A' | b'M' | b'D' | b'T' | b'R' | b'C' | b'U') {
            return Err("git diff returned an unknown name-status code".into());
        }
        let (path, previous) = if matches!(code, b'R' | b'C') {
            let destination = fields
                .next()
                .ok_or("git diff returned a rename without a destination")?;
            (
                destination,
                Some(String::from_utf8_lossy(path).into_owned()),
            )
        } else {
            (path, None)
        };
        files.push(ChangedFile {
            path: String::from_utf8_lossy(path).into_owned(),
            previous,
            status: code as char,
        });
    }
    Ok(files)
}

fn inspect_inner(
    root: &Path,
    remote_url: &str,
    branch: &str,
    delta: &mut Delta,
) -> Result<(), String> {
    if remote_url.is_empty() || branch.is_empty() || branch.starts_with('-') {
        return Err("default branch or remote URL is missing or invalid".into());
    }
    let mut matches = Vec::new();
    let remotes = text(root, &["remote"])?;
    for remote in remotes.lines() {
        let key = format!("remote.{remote}.url");
        if text(root, &["config", "--get", &key])? == remote_url {
            matches.push(remote);
        }
    }
    let remote = match matches.as_slice() {
        [] => {
            return Err(format!(
                "no local Git remote matches configured URL {remote_url}"
            ));
        }
        [remote] => *remote,
        _ => {
            return Err(format!(
                "multiple Git remotes match configured URL {remote_url}"
            ));
        }
    };
    let target_ref = format!("refs/remotes/{remote}/{branch}");
    if text(root, &["check-ref-format", &target_ref]).is_err() {
        return Err(format!("invalid default branch {branch}"));
    }
    delta.target_ref = target_ref;
    delta.target_oid = object_id(&text(
        root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", delta.target_ref),
        ],
    )?)?
    .to_string();
    delta.head_oid =
        object_id(&text(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?.to_string();
    let bases = text(
        root,
        &["merge-base", "--all", &delta.head_oid, &delta.target_oid],
    )?;
    let mut lines = bases.lines();
    let Some(base) = lines.next() else {
        return Err("HEAD and the default branch have no common ancestor".into());
    };
    if lines.next().is_some() {
        return Err("HEAD and the default branch have multiple merge bases".into());
    }
    delta.base_oid = object_id(base)?.to_string();
    let output = git(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "--find-renames",
            &delta.base_oid,
            &delta.head_oid,
            "--",
        ],
    )?;
    let committed = parse_names(&output.stdout)?;
    let current_ref = text(
        root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", delta.target_ref),
        ],
    )?;
    let current_head = text(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if current_ref != delta.target_oid || current_head != delta.head_oid {
        return Err(
            "HEAD or the cached default-branch ref moved during inspection; refresh the overview"
                .into(),
        );
    }
    delta.committed = committed;
    Ok(())
}

/// Compare HEAD to the merge-base with the configured remote's *local* default-branch ref.
/// Any unavailable/ambiguous baseline is an issue, not an empty clean result.
pub fn inspect(root: &Path, remote_url: &str, branch: &str) -> Delta {
    let mut delta = Delta::empty();
    if let Err(issue) = inspect_inner(root, remote_url, branch, &mut delta) {
        delta.issue = Some(issue);
        delta.committed.clear();
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_z_keeps_rename_source_and_special_paths() {
        let files = parse_names(b"R094\0before\tname\0after\nname\0M\0plain\0").unwrap();
        assert_eq!(
            files,
            vec![
                ChangedFile {
                    path: "after\nname".into(),
                    previous: Some("before\tname".into()),
                    status: 'R'
                },
                ChangedFile {
                    path: "plain".into(),
                    previous: None,
                    status: 'M'
                },
            ]
        );

        assert!(parse_names(b"R100\0before\0").is_err());
        assert!(parse_names(b"M\0file").is_err());
    }

    #[test]
    fn inspect_uses_local_default_branch_merge_base_not_the_index() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "herdr-delta-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        run(&["init", "-q"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@example.invalid"]);
        std::fs::write(root.join("old name"), "base\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "base"]);
        let base = run(&["rev-parse", "HEAD"]);
        run(&["remote", "add", "origin", "file:///example/default.git"]);
        run(&["update-ref", "refs/remotes/origin/main", &base]);
        std::fs::rename(root.join("old name"), root.join("new name")).unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "rename"]);
        let head = run(&["rev-parse", "HEAD"]);
        std::fs::write(root.join("new name"), "dirty worktree\n").unwrap();
        std::fs::write(root.join("staged only"), "index\n").unwrap();
        run(&["add", "staged only"]);

        let delta = inspect(&root, "file:///example/default.git", "main");
        assert_eq!(delta.issue, None);
        assert_eq!(delta.target_ref, "refs/remotes/origin/main");
        assert_eq!(delta.target_oid, base);
        assert_eq!(delta.base_oid, base);
        assert_eq!(delta.head_oid, head);
        assert_eq!(
            delta.committed,
            vec![ChangedFile {
                path: "new name".into(),
                previous: Some("old name".into()),
                status: 'R',
            }]
        );
        assert!(
            inspect(&root, "file:///other/default.git", "main")
                .issue
                .is_some()
        );
        assert!(
            inspect(&root, "file:///example/default.git", "missing")
                .issue
                .is_some()
        );
        let tree = run(&["rev-parse", "HEAD^{tree}"]);
        let unrelated = run(&["commit-tree", &tree, "-m", "unrelated root"]);
        run(&["update-ref", "refs/remotes/origin/main", &unrelated]);
        let unrelated_delta = inspect(&root, "file:///example/default.git", "main");
        assert_eq!(
            unrelated_delta.issue.as_deref(),
            Some("HEAD and the default branch have no common ancestor")
        );
        assert!(unrelated_delta.committed.is_empty());
        let left = run(&["commit-tree", &tree, "-p", &base, "-m", "left"]);
        let right = run(&["commit-tree", &tree, "-p", &base, "-m", "right"]);
        let left_merge = run(&[
            "commit-tree",
            &tree,
            "-p",
            &left,
            "-p",
            &right,
            "-m",
            "left merge",
        ]);
        let right_merge = run(&[
            "commit-tree",
            &tree,
            "-p",
            &right,
            "-p",
            &left,
            "-m",
            "right merge",
        ]);
        run(&["update-ref", "HEAD", &left_merge]);
        run(&["update-ref", "refs/remotes/origin/main", &right_merge]);
        let ambiguous_delta = inspect(&root, "file:///example/default.git", "main");
        assert_eq!(
            ambiguous_delta.issue.as_deref(),
            Some("HEAD and the default branch have multiple merge bases")
        );
        assert!(ambiguous_delta.committed.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
