//! Path checks for anything that names a file on the untrusted side.
//!
//! Adapted from provably.fast @ 4ad7a2b8: `validate_canonical_path(s)`
//! (crates/artifacts/src/lib.rs:1716-1756) and `valid_edit_surface`
//! (crates/candidates/src/lib.rs:431-445), merged into one check.

const MAX_PATH_BYTES: usize = 4096;

/// A relative path of `[A-Za-z0-9._-/]` with no empty, `.` or `..` components.
/// Rules out absolute paths, backslashes, `//` and trailing `/`.
pub fn is_canonical(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Every path is canonical and sits at or under one of the `editable` roots.
#[allow(dead_code)] // used by preflight and the PR gate
pub fn within(paths: &[&str], editable: &[&str]) -> bool {
    paths.iter().all(|p| {
        is_canonical(p)
            && editable.iter().any(|root| {
                let root = root.trim_end_matches('/');
                *p == root
                    || p.strip_prefix(root)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical() {
        for ok in ["candidate/src/lib.rs", "a", "a-b_c.d/e"] {
            assert!(is_canonical(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "a//b",
            "a/",
            "./a",
            "a/../b",
            "..",
            "a\\b",
            "a b",
            "é",
        ] {
            assert!(!is_canonical(bad), "{bad}");
        }
    }

    #[test]
    fn editable_surface() {
        let roots = ["candidate/"];
        assert!(within(&["candidate/src/lib.rs", "candidate"], &roots));
        assert!(!within(&["candidates/x.rs"], &roots));
        assert!(!within(&["candidate/../verifier/run"], &roots));
        assert!(!within(&["bench/run"], &roots));
    }
}
