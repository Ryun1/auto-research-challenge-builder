//! Token policy for Rust-kit candidates (PLAN.md §7a).
//!
//! Lexical, like provably.fast `public_preflight.py:327-442`
//! (`_strip_rust_noncode` + token counting), ported and extended with the
//! tokens the audit found missing. Comments, strings and char literals are
//! blanked first, so a banned word in a comment is fine.
//!
//! Macro expansion is not seen here; the kit's non-editable
//! `[lints.rust] unsafe_code = "forbid"` is the compiler-side backstop.

use std::path::Path;

/// Opt-in groups (`[rust] allow = [...]` in challenge.toml).
pub const OPT_IN: [&str; 3] = ["unsafe", "asm", "target_feature"];

/// (identifier, needs `!` after it, group). Groups outside [`OPT_IN`] are
/// never allowed: the candidate is pure compute.
const RULES: &[(&str, bool, &str)] = &[
    ("unsafe", false, "unsafe"),
    ("asm", true, "asm"),
    ("global_asm", true, "asm"),
    ("naked_asm", true, "asm"),
    ("target_feature", false, "target_feature"),
    // Compile-time reads of the judge's files or environment.
    ("include", true, "compile-time read"),
    ("include_bytes", true, "compile-time read"),
    ("include_str", true, "compile-time read"),
    ("env", false, "environment"),
    ("option_env", true, "environment"),
    // Symbol and linkage games (e.g. redefining the measured region).
    ("extern", false, "linkage"),
    ("no_mangle", false, "linkage"),
    ("export_name", false, "linkage"),
    ("link_section", false, "linkage"),
    ("link", false, "linkage"),
    ("global_allocator", false, "linkage"),
    // I/O and concurrency: could read the seed/oracle or work off the clock.
    ("fs", false, "io"),
    ("net", false, "io"),
    ("process", false, "io"),
    ("thread", false, "io"),
    ("File", false, "io"),
    ("Command", false, "io"),
];

/// Replaces comments, string/byte/raw-string and char literals with spaces,
/// keeping newlines so line numbers survive.
pub fn strip(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
        } else if (c == 'r' || (c == 'b' && next == Some('r')))
            && !prev_is_ident(&b, i)
            && raw_start(&b, i + if c == 'b' { 2 } else { 1 }).is_some()
        {
            let start = i + if c == 'b' { 2 } else { 1 };
            let hashes = raw_start(&b, start).unwrap();
            let close: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            let mut j = start + hashes + 1;
            while j < b.len() && b[j..].iter().take(close.len()).ne(close.iter()) {
                j += 1;
            }
            let end = (j + close.len()).min(b.len());
            b[i..end].iter().for_each(|&ch| out.push(blank(ch)));
            i = end;
        } else if c == '"' || (c == 'b' && next == Some('"') && !prev_is_ident(&b, i)) {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            while j < b.len() && b[j] != '"' {
                j += if b[j] == '\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(b.len());
            b[i..end].iter().for_each(|&ch| out.push(blank(ch)));
            i = end;
        } else if c == '\'' {
            // Char literal ('a', '\n', '\u{..}') vs lifetime ('a).
            let end = if next == Some('\\') {
                b[i + 2..]
                    .iter()
                    .position(|&ch| ch == '\'')
                    .map(|p| i + 2 + p + 1)
            } else if b.get(i + 2) == Some(&'\'') {
                Some(i + 3)
            } else {
                None
            };
            match end {
                Some(e) => {
                    b[i..e].iter().for_each(|&ch| out.push(blank(ch)));
                    i = e;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn prev_is_ident(b: &[char], i: usize) -> bool {
    i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_')
}

/// `#*"` at `at`: the number of hashes of a raw string opener.
fn raw_start(b: &[char], at: usize) -> Option<usize> {
    let hashes = b[at.min(b.len())..]
        .iter()
        .take_while(|&&c| c == '#')
        .count();
    (b.get(at + hashes) == Some(&'"')).then_some(hashes)
}

/// Every banned token in one file's source: `(line, message)`.
pub fn scan(src: &str, allow: &[String]) -> Vec<(usize, String)> {
    let code = strip(src);
    let mut hits = Vec::new();
    for (n, line) in code.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                // `r#unsafe` is a raw identifier, not the keyword.
                let raw_ident = start >= 2 && chars[start - 1] == '#' && chars[start - 2] == 'r';
                let bang = chars[i..].iter().find(|c| !c.is_whitespace()) == Some(&'!');
                // `#[path = "..."]` points a module at a file outside src/.
                let before: String = chars[..start]
                    .iter()
                    .rev()
                    .filter(|c| !c.is_whitespace())
                    .take(2)
                    .collect();
                if ident == "path" && (before == "[#" || before == "[!") {
                    hits.push((
                        n + 1,
                        "`#[path]` not allowed: modules must live under src/".into(),
                    ));
                }
                for (tok, needs_bang, group) in RULES {
                    if ident == *tok
                        && (!needs_bang || bang)
                        && !raw_ident
                        && !allow.iter().any(|a| a == group)
                    {
                        let hint = if OPT_IN.contains(group) {
                            format!(" (opt in with [rust] allow = [\"{group}\"])")
                        } else {
                            String::new()
                        };
                        hits.push((
                            n + 1,
                            format!(
                                "`{tok}{}` not allowed: {group}{hint}",
                                if *needs_bang { "!" } else { "" }
                            ),
                        ));
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    hits
}

/// Scans every file under `dir` (the editable surface). Non-`.rs` files are
/// refused too: a candidate crate's src/ holds Rust only.
pub fn check_dir(dir: &Path, allow: &[String]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) => return Err(vec![format!("{}: {e}", d.display())]),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            let ft = entry.file_type().map_err(|e| vec![e.to_string()])?;
            if ft.is_symlink() {
                errors.push(format!("{rel}: symlink"));
            } else if ft.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                match std::fs::read_to_string(&path) {
                    Ok(src) => errors.extend(
                        scan(&src, allow)
                            .into_iter()
                            .map(|(l, m)| format!("{rel}:{l}: {m}")),
                    ),
                    Err(e) => errors.push(format!("{rel}: {e}")),
                }
            } else if path.file_name().is_some_and(|n| n != "README.md") {
                errors.push(format!(
                    "{rel}: only .rs files belong in the candidate crate's src/"
                ));
            }
        }
    }
    errors.sort();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(src: &str) -> Vec<String> {
        scan(src, &[]).into_iter().map(|(_, m)| m).collect()
    }

    #[test]
    fn comments_strings_chars_are_blanked() {
        let src = r####"
// unsafe in a comment
/* nested /* unsafe */ still comment unsafe */
let s = "unsafe \" include_str!";
let r = r#"unsafe "quoted" env"#;
let b = br##"asm!"##;
let c = '"'; let d = '\''; let l: &'static str = "x";
fn ok<'a>(x: &'a u8) -> u8 { *x }
"####;
        assert!(hits(src).is_empty(), "{:?}", hits(src));
        assert_eq!(strip(src).lines().count(), src.lines().count());
    }

    #[test]
    fn banned_tokens() {
        for (src, want) in [
            ("unsafe { x() }", "unsafe"),
            ("core::arch::asm!(\"nop\")", "asm!"),
            ("#[target_feature(enable = \"avx2\")]", "target_feature"),
            ("if cfg!(target_feature = \"avx2\") {}", "target_feature"),
            (
                "const T: &[u8] = include_bytes!(\"../../oracle/x\");",
                "include_bytes!",
            ),
            ("let s = std::env::var(\"CB_SEED\");", "env"),
            ("let s = env!(\"HOME\");", "env"),
            ("#[path = \"../../bench/x.rs\"] mod m;", "#[path"),
            ("extern \"C\" { fn f(); }", "extern"),
            ("#[no_mangle] pub fn cb_measured() {}", "no_mangle"),
            ("std::fs::read(\"/tmp/seed\")", "fs"),
            ("std::thread::spawn(|| ())", "thread"),
        ] {
            let got = hits(src);
            assert!(
                got.iter().any(|m| m.contains(&format!("`{want}"))),
                "{src}: {got:?}"
            );
        }
    }

    #[test]
    fn allow_groups_and_raw_idents() {
        assert!(scan("unsafe { x() }", &["unsafe".into()]).is_empty());
        assert!(!scan("unsafe { std::fs::read(p) }", &["unsafe".into()]).is_empty());
        assert!(hits("let r#unsafe = 1;").is_empty());
        assert!(hits("let unsafe_count = 1; let environment = 2;").is_empty()); // whole idents only
        assert!(hits("let include = 1;").is_empty()); // no `!`
        assert!(hits("let path = p; fn f(path: &str) {}").is_empty()); // only the attribute
    }

    #[test]
    fn line_numbers() {
        let got = scan("fn a() {}\n// ok\nunsafe fn b() {}\n", &[]);
        assert_eq!(got[0].0, 3);
    }
}
