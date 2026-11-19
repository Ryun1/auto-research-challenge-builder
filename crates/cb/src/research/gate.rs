//! The research PR gate (design spec §3). Trusted: it reads the PR head only
//! through git objects and never executes anything from it.

use super::results;
use super::spec::{Declaration, ResearchSpec};
use crate::challenge::is_slug;
use crate::paths;
use crate::pipeline::{self, Gate};
use std::collections::BTreeSet;
use std::path::Path;

const MAX_DECL_BYTES: u64 = 64 * 1024;

/// Applies spec §3 to `merge_base..head`, collecting every refusal into
/// `g.errors`; exports the submission dir only when there are none.
#[allow(clippy::too_many_arguments)]
pub fn check(
    g: &mut Gate,
    repo: &Path,
    root: &str,
    slug: &str,
    spec: &ResearchSpec,
    merge_base: &str,
    base: &str,
    head: &str,
    out: &Path,
) {
    let prefix = format!("{root}/{slug}/submissions/");
    let raw = match pipeline::git(
        repo,
        &["diff", "--raw", "-z", "--no-renames", merge_base, head],
    ) {
        Ok(r) => r,
        Err(e) => {
            g.errors.push(e);
            return;
        }
    };
    // `:old new oldsha newsha S\0path\0` pairs.
    let toks: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|t| !t.is_empty())
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .collect();
    let mut ids = BTreeSet::new();
    let mut i = 0;
    while i + 1 < toks.len() {
        let status = toks[i].split_whitespace().last().unwrap_or("");
        let path = &toks[i + 1];
        i += 2;
        let id_and_rest = path.strip_prefix(&prefix).and_then(|r| r.split_once('/'));
        match id_and_rest {
            Some((id, rest)) if !id.is_empty() && !rest.is_empty() => {
                ids.insert(id.to_string());
                if status != "A" {
                    g.errors.push(only_add(path, root, slug));
                }
            }
            _ => g.errors.push(only_add(path, root, slug)),
        }
    }
    let listed: Vec<&String> = ids.iter().collect();
    let id = match listed.as_slice() {
        [id] => (*id).clone(),
        [] => {
            g.errors
                .push("no submission directory is added by this PR".into());
            return;
        }
        many => {
            g.errors.push(format!(
                "a PR must add exactly one submission dir; found {}",
                many.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            return;
        }
    };
    if !is_slug(&id) {
        g.errors.push(format!(
            "submission dir {id:?}: use lowercase a-z, 0-9 and -"
        ));
        return;
    }
    g.submission = Some(id.clone());
    let dir = format!("{prefix}{id}");

    // Merged submissions are immutable: the dir must not exist on main.
    for (what, rev) in [("the merge-base", merge_base), ("main", base)] {
        match pipeline::git(repo, &["ls-tree", "-z", rev, "--", &dir]) {
            Ok(l) if !l.is_empty() => g.errors.push(format!(
                "submission {id:?} already exists at {what}: submissions are immutable, add a new directory"
            )),
            Ok(_) => {}
            Err(e) => g.errors.push(e),
        }
    }
    if spec.challenge.status != "open" {
        g.errors.push(format!(
            "challenge status is {:?}, not open",
            spec.challenge.status
        ));
    }

    // The head tree of the dir: modes, canonical paths, size, required files.
    let listing = match pipeline::git(repo, &["ls-tree", "-r", "-l", "-z", head, "--", &dir]) {
        Ok(l) => l,
        Err(e) => {
            g.errors.push(e);
            return;
        }
    };
    let mut bytes = 0u64;
    let (mut toml_ok, mut build_ok) = (false, false);
    for entry in listing.split(|b| *b == 0).filter(|l| !l.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let (meta, path) = entry.split_once('\t').unwrap_or((&entry, ""));
        let f: Vec<&str> = meta.split_whitespace().collect();
        let mode = f.first().copied().unwrap_or("");
        match mode {
            "100644" | "100755" => {
                bytes += f.get(3).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
            }
            "120000" => g.errors.push(format!("{path}: symlink in a submission")),
            "160000" => g
                .errors
                .push(format!("{path}: gitlink (submodule) in a submission")),
            m => g.errors.push(format!("{path}: mode {m} in a submission")),
        }
        if !paths::is_canonical(path) {
            g.errors.push(format!("{path}: path not canonical"));
        }
        if path == format!("{dir}/submission.toml") {
            toml_ok = mode == "100644";
        }
        if path == format!("{dir}/build.sh") {
            build_ok = mode == "100755";
        }
    }
    if bytes > spec.submissions.max_bytes {
        g.errors.push(format!(
            "submission is {bytes} bytes > max_bytes {}",
            spec.submissions.max_bytes
        ));
    }
    if !toml_ok {
        g.errors
            .push("submission.toml must be present as a regular file (mode 100644)".into());
    }
    if !build_ok {
        g.errors
            .push("build.sh must be present as an executable regular file (mode 100755)".into());
    }

    // The declaration: read through git, never from the working tree.
    if toml_ok {
        read_declaration(g, repo, root, slug, spec, head, &dir, &id);
    }

    if g.errors.is_empty() {
        let _ = std::fs::remove_dir_all(out);
        if let Err(e) = pipeline::export_tar(repo, head, &dir, out) {
            g.errors.push(e);
        }
    }
}

fn only_add(path: &str, root: &str, slug: &str) -> String {
    format!(
        "{path}: a research submission may only add files under {root}/{slug}/submissions/<id>/"
    )
}

#[allow(clippy::too_many_arguments)]
fn read_declaration(
    g: &mut Gate,
    repo: &Path,
    root: &str,
    slug: &str,
    spec: &ResearchSpec,
    head: &str,
    dir: &str,
    id: &str,
) {
    let obj = format!("{head}:{dir}/submission.toml");
    let size = match pipeline::git_str(repo, &["cat-file", "-s", &obj]) {
        Ok(s) => s.parse::<u64>().unwrap_or(u64::MAX),
        Err(e) => {
            g.errors.push(e);
            return;
        }
    };
    if size > MAX_DECL_BYTES {
        g.errors.push(format!(
            "submission.toml is {size} bytes, at most {MAX_DECL_BYTES}"
        ));
        return;
    }
    let text = match pipeline::git(repo, &["show", &obj]) {
        Ok(t) => String::from_utf8_lossy(&t).into_owned(),
        Err(e) => {
            g.errors.push(e);
            return;
        }
    };
    let decl = match Declaration::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            g.errors.push(format!("submission.toml: {e}"));
            return;
        }
    };
    g.errors.extend(
        decl.check(spec, id)
            .into_iter()
            .map(|e| format!("submission.toml: {e}")),
    );
    // Lineage, against the checkout's results.
    let rows = match pipeline::read_results(&repo.join(root), slug) {
        Ok(r) => r,
        Err(e) => {
            g.errors.push(e);
            return;
        }
    };
    let accepted = results::accepted(&rows);
    match decl.novelty.as_str() {
        "builds-on" if is_slug(&decl.parent) => {
            let exists = repo
                .join(root)
                .join(slug)
                .join("submissions")
                .join(&decl.parent)
                .is_dir();
            if !exists {
                g.errors.push(format!(
                    "parent {} is not accepted on main: no such submission",
                    decl.parent
                ));
            } else if !accepted.contains_key(&decl.parent) {
                g.errors.push(format!(
                    "parent {} is not accepted on main yet (a just-merged parent is measured first: re-run this PR after its accept run records it)",
                    decl.parent
                ));
            }
        }
        "new" => {
            if let Some((other, _)) = accepted
                .iter()
                .find(|(_, r)| r["direction"].as_str() == Some(decl.direction.as_str()))
            {
                g.errors.push(format!(
                    "direction {:?} is already taken by accepted submission {other:?}: build on it or pick a new direction",
                    decl.direction
                ));
            }
        }
        _ => {}
    }
    g.declaration = Some(decl.to_json());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::tests::sh;
    use crate::pipeline::{gate, git_str};
    use crate::research::scaffold::scaffold;

    /// An open research challenge `toy` committed on main with an accepted
    /// baseline row and a 4096 byte submission cap.
    fn repo() -> std::path::PathBuf {
        let dir = paths::scratch("rgate");
        let ch = scaffold(&dir.join("challenges"), "toy").unwrap();
        let spec = std::fs::read_to_string(ch.join("challenge.toml"))
            .unwrap()
            .replace("status = \"draft\"", "status = \"open\"")
            .replace("max_bytes  = 16777216", "max_bytes  = 4096");
        std::fs::write(ch.join("challenge.toml"), spec).unwrap();
        std::fs::write(
            ch.join("results.jsonl"),
            "{\"stage\":\"baseline\",\"status\":\"accepted\",\"submission\":\"baseline\",\"direction\":\"baseline\"}\n",
        )
        .unwrap();
        sh(
            &dir,
            "git init -q -b main && git add -A && git commit -qm base",
        );
        dir
    }

    fn decl(id: &str) -> String {
        format!(
            "id = \"{id}\"\ncontract = \"main@1\"\nnovelty = \"new\"\ndirection = \"{id}\"\ntaxonomy = {{ approach = \"other\" }}\nsummary = \"s\"\n"
        )
    }

    const TOML: &str = "challenges/toy/submissions/s1/submission.toml";

    /// Script adding `challenges/toy/submissions/<id>/` with a valid
    /// declaration and build.sh, then running `more` (each step ends `&&`).
    fn add(id: &str, more: &str) -> String {
        let d = format!("challenges/toy/submissions/{id}");
        format!(
            "mkdir -p {d} && cat > {d}/submission.toml <<'EOT'\n{}EOT\nprintf '#!/bin/sh\\n' > {d}/build.sh && chmod 755 {d}/build.sh && {more}true",
            decl(id)
        )
    }

    /// perl in-place edit of s1's declaration.
    fn edit_toml(expr: &str) -> String {
        format!("perl -pi -e '{expr}' {TOML} &&")
    }

    fn run(dir: &Path, script: &str) -> (Gate, std::path::PathBuf, String) {
        sh(
            dir,
            &format!(
                "git checkout -q --detach main && {script} && git add -A && git commit -qm pr"
            ),
        );
        let h = git_str(dir, &["rev-parse", "HEAD"]).unwrap();
        let out = paths::scratch("rgate-out");
        let g = gate(dir, "challenges", "main", &h, &out);
        (g, out, h)
    }

    fn refused(script: &str, want: &str) {
        let dir = repo();
        let (g, out, _) = run(&dir, script);
        assert!(
            g.errors.iter().any(|e| e.contains(want)),
            "want {want:?}, got {:?}",
            g.errors
        );
        assert!(!out.exists(), "exported despite errors");
    }

    #[test]
    fn adds_one_submission_ok() {
        let dir = repo();
        let (g, out, h) = run(&dir, &add("s1", ""));
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert!(out
            .join("challenges/toy/submissions/s1/submission.toml")
            .exists());
        let j = g.json("main", &h);
        assert_eq!(j["type"], "research");
        assert_eq!(j["submission"], "s1");
        assert_eq!(j["declaration"]["id"], "s1");
        assert_eq!(j["ok"], true);
    }

    #[test]
    fn touches_outside_dir() {
        refused(
            &add("s1", "echo x >> challenges/toy/README.md &&"),
            "may only add files under",
        );
    }

    #[test]
    fn writes_measure_json() {
        refused(
            &add("s1", "echo '{}' > challenges/toy/measure.json &&"),
            "may only add files under",
        );
    }

    #[test]
    fn edits_judge() {
        refused(
            &add("s1", "echo x >> challenges/toy/contracts/main/judge &&"),
            "may only add files under",
        );
    }

    #[test]
    fn edits_existing_submission() {
        let s = "echo '# x' >> challenges/toy/submissions/baseline/build.sh";
        refused(s, "may only add files under");
        refused(s, "already exists");
    }

    #[test]
    fn adds_file_to_existing_submission() {
        refused(
            "echo x > challenges/toy/submissions/baseline/new.txt",
            "already exists",
        );
    }

    #[test]
    fn deletes_a_file() {
        refused(
            &add(
                "s1",
                "git rm -q challenges/toy/submissions/baseline/README.md &&",
            ),
            "may only add files under",
        );
    }

    #[test]
    fn two_submission_dirs() {
        let s = format!("{} && {}", add("s1", ""), add("s2", ""));
        refused(&s, "exactly one submission");
    }

    #[test]
    fn symlink_in_dir() {
        refused(
            &add(
                "s1",
                "ln -s /etc/passwd challenges/toy/submissions/s1/link &&",
            ),
            "symlink",
        );
    }

    #[test]
    fn gitlink_in_dir() {
        refused(
            &add(
                "s1",
                "git init -q challenges/toy/submissions/s1/sub && git -C challenges/toy/submissions/s1/sub commit -q --allow-empty -m x &&",
            ),
            "gitlink",
        );
    }

    #[test]
    fn size_cap() {
        refused(
            &add(
                "s1",
                "head -c 5000 /dev/zero > challenges/toy/submissions/s1/big &&",
            ),
            "bytes > max_bytes",
        );
    }

    #[test]
    fn missing_build_sh() {
        refused(
            &add(
                "s1",
                "find challenges/toy/submissions/s1 -name build.sh -delete &&",
            ),
            "build.sh",
        );
    }

    #[test]
    fn build_sh_not_executable() {
        refused(
            &add("s1", "chmod 644 challenges/toy/submissions/s1/build.sh &&"),
            "build.sh",
        );
    }

    #[test]
    fn forged_id() {
        refused(
            &add("s1", &edit_toml("s/^id = .*/id = \"baseline\"/")),
            "doesn't match directory",
        );
    }

    #[test]
    fn bad_toml_is_refused() {
        refused(
            &add("s1", &format!("echo 'nonsense =' > {TOML} &&")),
            "submission.toml",
        );
    }

    #[test]
    fn non_slug_id() {
        refused(
            "mkdir -p challenges/toy/submissions/Bad_Id && echo x > challenges/toy/submissions/Bad_Id/f",
            "Bad_Id",
        );
    }

    fn builds_on(parent: &str) -> String {
        edit_toml(&format!(
            "s/^novelty = .*/novelty = \"builds-on\"/; s/^direction = .*/parent = \"{parent}\"/"
        ))
    }

    #[test]
    fn unknown_parent() {
        refused(&add("s1", &builds_on("nope")), "not accepted on main");
    }

    #[test]
    fn parent_not_accepted() {
        let dir = repo();
        sh(
            &dir,
            "mkdir -p challenges/toy/submissions/p && echo x > challenges/toy/submissions/p/f && git add -A && git commit -qm p",
        );
        let (g, _, _) = run(&dir, &add("s1", &builds_on("p")));
        assert!(
            g.errors
                .iter()
                .any(|e| e.contains("not accepted on main yet")),
            "{:?}",
            g.errors
        );
    }

    #[test]
    fn builds_on_accepted_parent_ok() {
        let dir = repo();
        let (g, _, _) = run(&dir, &add("s1", &builds_on("baseline")));
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.declaration.unwrap()["parent"], "baseline");
    }

    #[test]
    fn direction_taken() {
        refused(
            &add(
                "s1",
                &edit_toml("s/^direction = .*/direction = \"baseline\"/"),
            ),
            "direction",
        );
    }

    #[test]
    fn deferred_contract() {
        refused(
            &add("s1", &edit_toml("s/main\\@1/streaming\\@0/")),
            "deferred",
        );
    }

    #[test]
    fn challenge_not_open() {
        let dir = repo();
        sh(
            &dir,
            "perl -pi -e 's/^status = \"open\"/status = \"closed\"/' challenges/toy/challenge.toml && git add -A && git commit -qm close",
        );
        let (g, _, _) = run(&dir, &add("s1", ""));
        assert!(
            g.errors.iter().any(|e| e.contains("not open")),
            "{:?}",
            g.errors
        );
    }

    #[test]
    fn simple_gate_reports_type() {
        let dir = crate::pipeline::tests::repo();
        let h = crate::pipeline::tests::head(&dir, "echo 60 > challenges/toy/candidate/answer");
        let out = paths::scratch("sgate-out");
        let g = gate(&dir, "challenges", "main", &h, &out);
        assert_eq!(g.json("main", &h)["type"], "simple");
    }
}
