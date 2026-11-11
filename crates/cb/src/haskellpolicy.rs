//! Source policy for Haskell candidates (issue #51), diffed against the baseline.
//!
//! An allowlist, not a denylist. GHC pragma names are case-insensitive and
//! `{-#ANN` needs no space, so plain substring bans miss them. This module lexes
//! the source the way GHC does (comments, strings and char literals are not code),
//! then checks, per file against the same baseline file:
//!
//! - **Pragmas.** Optimisation and declaration pragmas (`INLINE`, `UNPACK`, ...)
//!   are fine anywhere. `LANGUAGE` extensions must be on a safe list or already
//!   on in that baseline file, and a few are never allowed (`MagicHash`, TH,
//!   FFI, ...). Every other pragma (`OPTIONS_GHC`, `ANN`, `RULES`, `INCLUDE`, ...)
//!   must appear verbatim in that baseline file.
//! - **CPP.** Directive lines must each appear in that baseline file.
//! - **Imports.** A module must be package-local, on a safe list, or imported
//!   by the baseline. Modules that reach memory, IO or GHC internals must match
//!   a baseline import declaration exactly (same import list). Some are never
//!   allowed (`GHC.Exts`, `GHC.Prim`, `System.IO`, ...).
//! - **Tokens.** No `foreign` declarations. Identifiers containing `unsafe`
//!   (any case), `runRW#` and friends are counted per identifier and may not
//!   exceed the baseline's counts.
//! - **Files.** Only `src/` may differ from the baseline, and `src/` has the
//!   same file set (the cabal file is frozen, so a new module couldn't build).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Pragmas that only steer optimisation or declare things. Safe anywhere.
const SAFE_PRAGMAS: &[&str] = &[
    "INLINE",
    "INLINABLE",
    "INLINEABLE",
    "NOINLINE",
    "OPAQUE",
    "SPECIALISE",
    "SPECIALIZE",
    "UNPACK",
    "NOUNPACK",
    "COMPLETE",
    "MINIMAL",
    "OVERLAPPING",
    "OVERLAPPABLE",
    "OVERLAPS",
    "INCOHERENT",
    "DEPRECATED",
    "WARNING",
    "SCC",
];

/// Extensions with no route to memory, IO or compile-time code execution.
const SAFE_EXTENSIONS: &[&str] = &[
    "AllowAmbiguousTypes",
    "BangPatterns",
    "BinaryLiterals",
    "BlockArguments",
    "ConstraintKinds",
    "DataKinds",
    "DefaultSignatures",
    "DeriveAnyClass",
    "DeriveFoldable",
    "DeriveFunctor",
    "DeriveGeneric",
    "DeriveTraversable",
    "DerivingStrategies",
    "DerivingVia",
    "DuplicateRecordFields",
    "EmptyCase",
    "ExistentialQuantification",
    "FlexibleContexts",
    "FlexibleInstances",
    "FunctionalDependencies",
    "GADTs",
    "GeneralizedNewtypeDeriving",
    "InstanceSigs",
    "KindSignatures",
    "LambdaCase",
    "MultiParamTypeClasses",
    "MultiWayIf",
    "NamedFieldPuns",
    "NoStarIsType",
    "NumericUnderscores",
    "OverloadedRecordDot",
    "OverloadedStrings",
    "PatternGuards",
    "PatternSynonyms",
    "PolyKinds",
    "RankNTypes",
    "RecordWildCards",
    "ScopedTypeVariables",
    "StandaloneDeriving",
    "Strict",
    "StrictData",
    "TupleSections",
    "TypeApplications",
    "TypeData",
    "TypeFamilies",
    "TypeOperators",
    "UndecidableInstances",
    "ViewPatterns",
];

/// Raw memory and primops: allowed only in a file whose baseline already
/// enables them (e.g. cardano-ledger-core's Coin.hs uses UnboxedTuples).
const BASELINE_ONLY_EXTENSIONS: &[&str] = &[
    "MagicHash",
    "UnboxedTuples",
    "UnboxedSums",
    "UnliftedNewtypes",
    "UnliftedDatatypes",
];

/// Never allowed, even if the baseline file has them: compile-time code
/// (TH), FFI, and Safe Haskell escape hatches.
const BANNED_EXTENSIONS: &[&str] = &[
    "TemplateHaskell",
    "TemplateHaskellQuotes",
    "QuasiQuotes",
    "UnliftedFFITypes",
    "GHCForeignImportPrim",
    "ForeignFunctionInterface",
    "CApiFFI",
    "InterruptibleFFI",
    "StaticPointers",
    "PackageImports",
    "Unsafe",
    "Trustworthy",
];

/// Modules that reach primops, raw memory, IO or processes: allowed only as
/// an exact copy of an import declaration in the same baseline file (upstream
/// has e.g. `import GHC.Base (Type)` and `import Debug.Trace (trace)`).
const BANNED_MODULES: &[&str] = &[
    "GHC.Exts",
    "GHC.Prim",
    "GHC.Base",
    "GHC.Magic",
    "GHC.ST",
    "GHC.IO",
    "GHC.IORef",
    "GHC.Conc",
    "GHC.Ptr",
    "GHC.ForeignPtr",
    "GHC.Weak",
    "GHC.Stable",
    "GHC.StableName",
    "System.IO",
    "System.Process",
    "System.Environment",
    "System.Exit",
    "System.Posix",
    "System.Mem",
    "Data.IORef",
    "Debug.Trace",
    "Language.Haskell.TH",
    "Control.Concurrent",
    "Codec.CBOR.Magic",
];

/// Prefixes (whole module-name segments) that reach memory, IO or GHC
/// internals: allowed only as an exact copy of a baseline import declaration.
const SENSITIVE_PREFIXES: &[&str] = &[
    "GHC",
    "Foreign",
    "System",
    "Unsafe",
    "Data.Primitive",
    "Control.Monad.ST",
    "Control.Exception",
    "Data.Array",
    "Codec.CBOR.Read",
    "Codec.CBOR.ByteArray",
];

/// Plain pure modules a candidate may import even if the baseline doesn't.
const SAFE_MODULES: &[&str] = &[
    "Control.Applicative",
    "Control.Monad",
    "Data.Bits",
    "Data.Bool",
    "Data.Char",
    "Data.Coerce",
    "Data.Either",
    "Data.Foldable",
    "Data.Function",
    "Data.Functor",
    "Data.Int",
    "Data.Kind",
    "Data.List",
    "Data.List.NonEmpty",
    "Data.Maybe",
    "Data.Monoid",
    "Data.Ord",
    "Data.Proxy",
    "Data.Ratio",
    "Data.Semigroup",
    "Data.Traversable",
    "Data.Tuple",
    "Data.Void",
    "Data.Word",
    "Numeric.Natural",
];

/// Identifiers counted against the baseline (besides anything containing "unsafe").
const COUNTED: &[&str] = &[
    "runRW#",
    "realWorld#",
    "accursedUnutterablePerformIO",
    "inlinePerformIO",
];

/// A lexed source file.
#[derive(Debug, Default, PartialEq)]
pub struct Lexed {
    /// The code with comments, pragmas, strings and char literals blanked
    /// (newlines kept).
    pub code: String,
    /// Each `{-# ... #-}` pragma body, whitespace-normalised.
    pub pragmas: Vec<String>,
    /// Ended inside a string, comment or pragma, or a string GHC would
    /// reject: the split above can't be trusted, so the file is refused.
    pub malformed: bool,
}

fn is_symbol(c: char) -> bool {
    "!#$%&*+./<=>?@\\^|-~:".contains(c)
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits Haskell source into code and pragmas, the way GHC's lexer sees it.
pub fn lex(src: &str) -> Lexed {
    let b: Vec<char> = src.chars().collect();
    let mut out = Lexed::default();
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let at = |k: usize| b.get(i + k).copied();
        // {-# pragma #-} (not nested)
        if c == '{' && at(1) == Some('-') && at(2) == Some('#') {
            let start = i + 3;
            let mut j = start;
            while j + 2 < b.len() && !(b[j] == '#' && b[j + 1] == '-' && b[j + 2] == '}') {
                j += 1;
            }
            if j + 2 >= b.len() {
                out.malformed = true;
            }
            let end = j.min(b.len());
            out.pragmas
                .push(norm(&b[start..end].iter().collect::<String>()));
            let stop = (j + 3).min(b.len());
            out.code.extend(b[i..stop].iter().map(|&c| blank(c)));
            i = stop;
            continue;
        }
        // {- nested comment -}
        if c == '{' && at(1) == Some('-') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '{' && b.get(i + 1) == Some(&'-') {
                    depth += 1;
                    out.code.push_str("  ");
                    i += 2;
                } else if b[i] == '-' && b.get(i + 1) == Some(&'}') {
                    depth -= 1;
                    out.code.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.code.push(blank(b[i]));
                    i += 1;
                }
            }
            if depth > 0 {
                out.malformed = true;
            }
            continue;
        }
        // -- line comment: 2+ dashes not part of a longer operator
        if c == '-' && at(1) == Some('-') && !(i > 0 && is_symbol(b[i - 1])) {
            let mut j = i;
            while j < b.len() && b[j] == '-' {
                j += 1;
            }
            if j >= b.len() || !is_symbol(b[j]) {
                while i < b.len() && b[i] != '\n' {
                    out.code.push(' ');
                    i += 1;
                }
                continue;
            }
        }
        // "string" (escapes and gaps)
        if c == '"' {
            out.code.push(' ');
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\n' {
                    out.malformed = true; // GHC: lexical error
                }
                if b[i] != '\\' {
                    out.code.push(blank(b[i]));
                    i += 1;
                    continue;
                }
                match b.get(i + 1) {
                    // String gap: \ whitespace... \
                    Some(w) if w.is_whitespace() => {
                        out.code.push(' ');
                        i += 1;
                        while i < b.len() && b[i].is_whitespace() {
                            out.code.push(blank(b[i]));
                            i += 1;
                        }
                        if b.get(i) == Some(&'\\') {
                            out.code.push(' ');
                            i += 1;
                        } else {
                            out.malformed = true;
                        }
                    }
                    // \^X: X may be \ (control-backslash)
                    Some('^') => {
                        let stop = (i + 3).min(b.len());
                        out.code.extend(b[i..stop].iter().map(|&c| blank(c)));
                        i = stop;
                    }
                    // \" \\ \n \x41 \NUL ...: the rest of a long escape is
                    // alphanumeric, so consuming two chars is enough.
                    _ => {
                        let stop = (i + 2).min(b.len());
                        out.code.extend(b[i..stop].iter().map(|&c| blank(c)));
                        i = stop;
                    }
                }
            }
            if i < b.len() {
                out.code.push(' ');
                i += 1;
            } else {
                out.malformed = true;
            }
            continue;
        }
        // 'c' char literal (not a prime, not a promoted 'Constructor)
        if c == '\'' && !(i > 0 && is_ident(b[i - 1])) {
            let end = if at(1) == Some('\\') {
                // The escaped char (at i+2, maybe ' or \, or ^X) is never the end.
                let from = if at(2) == Some('^') { i + 4 } else { i + 3 };
                (from..b.len().min(i + 12)).find(|&k| b[k] == '\'')
            } else if at(2) == Some('\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(e) = end {
                out.code.extend(b[i..=e].iter().map(|&c| blank(c)));
                i = e + 1;
                continue;
            }
        }
        out.code.push(c);
        i += 1;
    }
    out
}

/// Pragma name (upper-cased) and its argument text.
fn pragma_parts(p: &str) -> (String, &str) {
    let name: String = p
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (name.to_uppercase(), p[name.len()..].trim())
}

/// Extensions named by the file's LANGUAGE pragmas (as written, minus "No").
fn extensions(l: &Lexed) -> Vec<String> {
    l.pragmas
        .iter()
        .map(|p| pragma_parts(p))
        .filter(|(n, _)| n == "LANGUAGE")
        .flat_map(|(_, args)| {
            args.split(',')
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Possible CPP directive lines, from the RAW source: CPP runs before GHC
/// lexes, so a directive inside a `{- -}` comment or a string gap still runs.
/// Any line whose first non-blank char is `#` (except a `#-}` pragma closer),
/// whitespace-normalised.
fn cpp_lines(raw: &str) -> Vec<String> {
    raw.split(['\n', '\r'])
        .map(str::trim_start)
        .filter(|ln| ln.starts_with('#') && !ln.starts_with("#-}"))
        .map(norm)
        .collect()
}

/// Import declarations: (module, whole declaration normalised).
fn imports(l: &Lexed) -> Vec<(String, String)> {
    // Every `import` keyword, wherever it is: after `;` or `{`, or in an
    // indented or explicit-brace module body, not just at column 0. It's a
    // reserved word, so outside comments and strings it always starts an
    // import (or a `foreign import`, refused separately). A declaration runs
    // to `;`, `}`, or a line indented no deeper than the keyword (layout).
    // Tabs count as one column here and as more for GHC, so this never takes
    // in text GHC leaves out. It can stop early, which only makes the
    // declaration fail to match.
    let b: Vec<char> = l.code.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 <= b.len() {
        let is_kw = b[i..i + 6].iter().collect::<String>() == "import"
            && (i == 0 || !(is_ident(b[i - 1]) || b[i - 1] == '.'))
            && b.get(i + 6).is_none_or(|&c| !is_ident(c));
        if !is_kw {
            i += 1;
            continue;
        }
        let col = i - b[..i].iter().rposition(|&c| c == '\n').map_or(0, |n| n + 1);
        let mut j = i;
        while j < b.len() && b[j] != ';' && b[j] != '}' {
            if b[j] == '\n' {
                // Next non-blank line: a continuation only if indented deeper.
                let mut k = j + 1;
                let mut ind = 0;
                loop {
                    match b.get(k) {
                        Some(' ') | Some('\t') => ind += 1,
                        Some('\n') => ind = 0,
                        _ => break,
                    }
                    k += 1;
                }
                if k >= b.len() || ind <= col {
                    break;
                }
            }
            j += 1;
        }
        let decl = norm(&b[i..j].iter().collect::<String>());
        let module = decl
            .split(|c: char| c.is_whitespace() || c == '(')
            .filter(|w| !w.is_empty())
            .skip(1)
            .find(|w| *w != "qualified" && *w != "safe")
            .unwrap_or("")
            .to_string();
        out.push((module, decl));
        i = j.max(i + 6);
    }
    out
}

/// Module name as a list of segments starts with `prefix`'s segments.
fn has_prefix(module: &str, prefix: &str) -> bool {
    module == prefix || module.starts_with(&format!("{prefix}."))
}

/// Counted identifiers: COUNTED ones and any containing "unsafe" (any case).
fn counted(code: &str) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    let b: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if is_ident(b[i]) || b[i] == '#' || b[i] == '.' {
            let s = i;
            while i < b.len() && (is_ident(b[i]) || b[i] == '#' || b[i] == '.') {
                i += 1;
            }
            let w: String = b[s..i].iter().collect();
            // Qualified names count by their last segment.
            let last = w.rsplit('.').next().unwrap_or(&w).to_string();
            if last.to_lowercase().contains("unsafe") || COUNTED.contains(&last.as_str()) {
                *m.entry(last).or_insert(0) += 1;
            }
        } else {
            i += 1;
        }
    }
    m
}

/// Has a `foreign` declaration (`foreign import` / `foreign export`).
fn has_foreign(code: &str) -> bool {
    let words: Vec<&str> = code
        .split(|c: char| !(is_ident(c) || c == '#'))
        .filter(|w| !w.is_empty())
        .collect();
    words
        .windows(2)
        .any(|w| w[0] == "foreign" && (w[1] == "import" || w[1] == "export"))
}

/// Checks one candidate file against its baseline counterpart.
/// `local` = modules defined by the package itself.
pub fn check_file(
    base: &str,
    cand: &str,
    local: &BTreeSet<String>,
    base_imports: &BTreeSet<String>,
    base_decls: &BTreeSet<String>,
) -> Vec<String> {
    let (lb, lc) = (lex(base), lex(cand));
    let mut errs = Vec::new();
    if lc.malformed {
        errs.push("unterminated or malformed string, comment or pragma".into());
    }
    // cpp and GHC may see line breaks Rust's lines() doesn't: refuse them.
    if cand.contains(['\x0b', '\x0c']) || cand.replace("\r\n", "").contains('\r') {
        errs.push("lone CR, vertical tab or form feed".into());
    }

    // Pragmas.
    let base_exts: BTreeSet<String> = extensions(&lb).into_iter().collect();
    let mut base_other: Vec<String> = lb
        .pragmas
        .iter()
        .filter(|p| {
            let (n, _) = pragma_parts(p);
            n != "LANGUAGE" && !SAFE_PRAGMAS.contains(&n.as_str())
        })
        .cloned()
        .collect();
    for p in &lc.pragmas {
        let (name, _) = pragma_parts(p);
        if name == "LANGUAGE" || SAFE_PRAGMAS.contains(&name.as_str()) {
            continue;
        }
        match base_other.iter().position(|q| q == p) {
            Some(k) => {
                base_other.remove(k);
            }
            None => errs.push(format!("pragma {{-# {p} #-}} is not in the baseline file")),
        }
    }
    for e in extensions(&lc) {
        let bare = e.strip_prefix("No").unwrap_or(&e);
        let is = |list: &[&str]| list.iter().any(|b| b.eq_ignore_ascii_case(bare));
        if is(BANNED_EXTENSIONS) {
            errs.push(format!("extension {e} is not allowed"));
        } else if is(BASELINE_ONLY_EXTENSIONS) {
            if !base_exts.contains(&e) {
                errs.push(format!(
                    "extension {e} is only allowed where the baseline file has it"
                ));
            }
        } else if !SAFE_EXTENSIONS.contains(&e.as_str()) && !base_exts.contains(&e) {
            errs.push(format!(
                "extension {e} is not on the safe list or in the baseline file"
            ));
        }
    }

    // CPP directives.
    let mut base_cpp = cpp_lines(base);
    for d in cpp_lines(cand) {
        match base_cpp.iter().position(|q| *q == d) {
            Some(k) => {
                base_cpp.remove(k);
            }
            None => errs.push(format!("CPP directive {d:?} is not in the baseline file")),
        }
    }

    // Imports.
    let same_file: BTreeSet<String> = imports(&lb).into_iter().map(|(_, d)| d).collect();
    for (m, decl) in imports(&lc) {
        if !m.starts_with(|c: char| c.is_ascii_uppercase()) {
            errs.push(format!("import not understood: {decl:?}"));
        } else if BANNED_MODULES.iter().any(|b| has_prefix(&m, b)) {
            if !same_file.contains(&decl) {
                errs.push(format!(
                    "import of {m} is only allowed as an exact copy of this baseline file's: {decl:?}"
                ));
            }
        } else if local.contains(&m) || SAFE_MODULES.contains(&m.as_str()) {
        } else if SENSITIVE_PREFIXES.iter().any(|p| has_prefix(&m, p))
            || m.ends_with(".Internal")
            || m.ends_with(".Unsafe")
            || m.contains(".Internal.")
        {
            if !base_decls.contains(&decl) {
                errs.push(format!(
                    "import of {m} must match a baseline import exactly: {decl:?}"
                ));
            }
        } else if !base_imports.contains(&m) {
            errs.push(format!(
                "import of {m}: not imported by the baseline, not package-local, not on the safe list"
            ));
        }
    }

    // Tokens.
    if has_foreign(&lc.code) {
        errs.push("foreign declaration".into());
    }
    let (cb, cc) = (counted(&lb.code), counted(&lc.code));
    for (k, n) in &cc {
        let was = cb.get(k).copied().unwrap_or(0);
        if *n > was {
            errs.push(format!("{k}: {n} uses > baseline {was}"));
        }
    }
    errs
}

fn walk(root: &Path) -> Result<BTreeMap<PathBuf, PathBuf>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let p = e.path();
            let ft = e.file_type().map_err(|e| e.to_string())?;
            let rel = p.strip_prefix(root).unwrap_or(&p).to_path_buf();
            if ft.is_dir() {
                stack.push(p);
            } else {
                out.insert(rel, p);
            }
        }
    }
    Ok(out)
}

fn module_name(rel_to_src: &Path) -> Option<String> {
    let s = rel_to_src.to_str()?.strip_suffix(".hs")?;
    Some(s.replace('/', "."))
}

/// Checks a candidate tree against the baseline tree. `srcs` are the source
/// directories (relative), the only parts that may change; each is a module
/// root.
pub fn check(base: &Path, cand: &Path, srcs: &[String]) -> Result<(), Vec<String>> {
    let fb = walk(base).map_err(|e| vec![e])?;
    let fc = walk(cand).map_err(|e| vec![e])?;
    let mut errs = Vec::new();
    let src_of = |r: &Path| srcs.iter().find(|s| r.starts_with(s.as_str())).cloned();
    let in_src = |r: &Path| src_of(r).is_some();
    let src = srcs.join(", ");
    for (rel, p) in &fc {
        if p.is_symlink() {
            errs.push(format!("{}: symlink", rel.display()));
        } else if !fb.contains_key(rel) {
            errs.push(format!(
                "{}: new file (the file set is frozen)",
                rel.display()
            ));
        } else if !in_src(rel) && std::fs::read(p).ok() != std::fs::read(&fb[rel]).ok() {
            errs.push(format!("{}: only {src}/ may change", rel.display()));
        }
    }
    for rel in fb.keys().filter(|r| !fc.contains_key(*r)) {
        errs.push(format!(
            "{}: deleted (the file set is frozen)",
            rel.display()
        ));
    }
    // Baseline import facts, package-wide.
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    let mut local = BTreeSet::new();
    let mut base_imports = BTreeSet::new();
    let mut base_decls = BTreeSet::new();
    for (rel, p) in fb.iter().filter(|(r, _)| in_src(r)) {
        if let Some(m) = src_of(rel)
            .and_then(|s| rel.strip_prefix(&s).ok().map(Path::to_path_buf))
            .and_then(|r| module_name(&r))
        {
            local.insert(m);
        }
        for (m, d) in imports(&lex(&read(p))) {
            base_imports.insert(m);
            base_decls.insert(d);
        }
    }
    for (rel, p) in fc.iter().filter(|(r, _)| in_src(r) && fb.contains_key(*r)) {
        if p.extension().is_none_or(|x| x != "hs") {
            if std::fs::read(p).ok() != std::fs::read(&fb[rel]).ok() {
                errs.push(format!("{}: only .hs files may change", rel.display()));
            }
            continue;
        }
        let Ok(c) = std::fs::read_to_string(p) else {
            errs.push(format!("{}: not UTF-8", rel.display()));
            continue;
        };
        errs.extend(
            check_file(&read(&fb[rel]), &c, &local, &base_imports, &base_decls)
                .into_iter()
                .map(|e| format!("{}: {e}", rel.display())),
        );
    }
    errs.sort();
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{-# LANGUAGE BangPatterns #-}
{-# LANGUAGE CPP #-}
{-# OPTIONS_GHC -Wno-orphans #-}
module M (f) where

import Data.Primitive.ByteArray (ByteArray)
import qualified Data.Map.Strict as Map
import Cardano.Ledger.Binary.Other (g)

#if MIN_VERSION_base(4,18,0)
x = 1
#endif

f :: Int -> Int
f !x = unsafeDeserialize x -- unsafe in a comment
{-# INLINE f #-}
"#;

    fn errs(cand: &str) -> Vec<String> {
        let base_lex = lex(BASE);
        let ims = imports(&base_lex);
        let local: BTreeSet<String> = ["M".into(), "Cardano.Ledger.Binary.Other".into()].into();
        check_file(
            BASE,
            cand,
            &local,
            &ims.iter().map(|(m, _)| m.clone()).collect(),
            &ims.iter().map(|(_, d)| d.clone()).collect(),
        )
    }

    fn with(extra_head: &str, extra_tail: &str) -> String {
        format!("{extra_head}{BASE}{extra_tail}")
    }

    #[test]
    fn baseline_passes_itself() {
        assert!(errs(BASE).is_empty(), "{:?}", errs(BASE));
    }

    #[test]
    fn pragma_spellings_issue_51() {
        // Lower-case, condensed and odd-spacing spellings of banned pragmas.
        for p in [
            "{-# options_ghc -F -pgmF ./pp.sh #-}\n",
            "{-#OPTIONS_GHC -F -pgmF ./pp.sh #-}\n",
            "{-#ANN module (\"x\" :: String) #-}\n",
            "{-# ann module () #-}\n",
            "{-#  OPTIONS  -cpp #-}\n",
            "{-# INCLUDE \"x.h\" #-}\n",
            "{-# RULES \"r\" forall x. f x = x #-}\n",
        ] {
            assert!(!errs(&with(p, "")).is_empty(), "{p}");
        }
        // The baseline's own OPTIONS_GHC, re-spaced, is still fine.
        let respaced = BASE.replace(
            "{-# OPTIONS_GHC -Wno-orphans #-}",
            "{-#OPTIONS_GHC   -Wno-orphans #-}",
        );
        assert!(errs(&respaced).is_empty(), "{:?}", errs(&respaced));
    }

    #[test]
    fn extension_rules() {
        for (p, bad) in [
            ("{-# LANGUAGE MagicHash, UnboxedTuples #-}\n", true),
            ("{-# language MagicHash #-}\n", true),
            ("{-# LANGUAGE TemplateHaskell #-}\n", true),
            ("{-# LANGUAGE ForeignFunctionInterface #-}\n", true),
            ("{-# LANGUAGE PackageImports #-}\n", true),
            ("{-# LANGUAGE Arrows #-}\n", true), // not listed, not in baseline
            ("{-# LANGUAGE ScopedTypeVariables, LambdaCase #-}\n", false),
        ] {
            assert_eq!(
                !errs(&with(p, "")).is_empty(),
                bad,
                "{p}: {:?}",
                errs(&with(p, ""))
            );
        }
    }

    #[test]
    fn cpp_lines_must_be_in_baseline() {
        assert!(!errs(&with("", "#include \"/etc/passwd\"\n")).is_empty());
        assert!(!errs(&with("", "# include \"x\"\n")).is_empty());
        assert!(!errs(&with("", "#define X 1\n")).is_empty());
        // Hidden from the Haskell lexer, not from CPP.
        assert!(!errs(&with("", "{-\n#include \"/etc/passwd\"\n-}\n")).is_empty());
        assert!(!errs(&with("", "s = \"a\\\n#include \"x\"\n\\b\"\n")).is_empty());
        assert!(!errs(&with("", "  #include \"x\"\n")).is_empty());
        assert!(!errs(&with("", "x = 1\r#include \"x\"\n")).is_empty());
        assert!(!errs(&with("", "x = 1\x0c\n")).is_empty());
        assert!(errs(&BASE.replace('\n', "\r\n")).is_empty());
        // A pragma closer on its own line is not a directive.
        assert!(errs(&with("", "{-# INLINE g\n  #-}\n")).is_empty());
    }

    #[test]
    fn import_rules() {
        for (imp, bad) in [
            ("import GHC.Exts\n", true),
            ("import GHC.Exts (Int (I#))\n", true),
            ("import qualified GHC.Prim as P\n", true),
            ("import System.IO.Unsafe (unsafePerformIO)\n", true),
            ("import Codec.CBOR.Magic (grabWord8)\n", true),
            ("import Data.ByteString.Internal (toForeignPtr)\n", true),
            ("import Foreign.Ptr (Ptr)\n", true),
            ("import Data.Primitive.Ptr (readOffPtr)\n", true),
            // Unrestricted form of a baseline import that was restricted.
            ("import Data.Primitive.ByteArray\n", true),
            ("import Data.Text (Text)\n", true), // not in baseline
            ("import Data.Primitive.ByteArray (ByteArray)\n", false),
            ("import qualified Data.Map.Strict as M2\n", false),
            ("import Data.Bits (shiftL, (.|.))\n", false),
            ("import Cardano.Ledger.Binary.Other\n", false),
        ] {
            let src = BASE.replacen("\nf ::", &format!("\n{imp}\nf ::"), 1);
            // Imports must come before decls in real code; here we only lex.
            assert_eq!(!errs(&src).is_empty(), bad, "{imp}: {:?}", errs(&src));
        }
    }

    #[test]
    fn banned_but_in_the_same_baseline_file() {
        let base = "{-# LANGUAGE UnboxedTuples #-}\nmodule M where\nimport GHC.Base (Type)\nimport Debug.Trace (trace)\n";
        let none = BTreeSet::new();
        let ck = |cand: &str| check_file(base, cand, &none, &none, &none);
        assert!(ck(base).is_empty(), "{:?}", ck(base));
        // Wider import list, or the extension's partner: refused.
        assert!(!ck(&base.replace("GHC.Base (Type)", "GHC.Base")).is_empty());
        assert!(!ck(&base.replace("(trace)", "(trace, traceIO)")).is_empty());
        assert!(!ck(&base.replace("UnboxedTuples", "UnboxedTuples, MagicHash")).is_empty());
        // Another file (no such baseline lines): refused.
        let other = check_file("module N where\n", base, &none, &none, &none);
        assert_eq!(other.len(), 3, "{other:?}");
    }

    #[test]
    fn imports_anywhere() {
        // Semicolons, explicit braces, an indented body, a split declaration.
        for src in [
            "module M where\nimport Data.List; import GHC.Exts\n",
            "module M where { import Data.List ; import GHC.Exts }\n",
            "module M where\n  import Data.List\n  import GHC.Exts\n",
            "module M where\nimport\n  GHC.Exts\n",
            "module M where\n    import Data.List\n\timport GHC.Exts\n",
        ] {
            let ms: Vec<String> = imports(&lex(src)).into_iter().map(|(m, _)| m).collect();
            assert!(ms.contains(&"GHC.Exts".to_string()), "{src:?}: {ms:?}");
            assert!(
                !errs(&with("", &src.replace("module M where", ""))).is_empty(),
                "{src:?}"
            );
        }
        // Not keywords: identifiers containing it, qualified names.
        assert!(imports(&lex("x = importer + M.import' + reimport\n")).is_empty());
        // A tab-indented continuation is cut short: the bare import can't
        // match the baseline's restricted one.
        let base = "module M where\n    import GHC.Base (Type)\n";
        let none = BTreeSet::new();
        let cand = "module M where\n    import GHC.Base\n\t(Type)\n";
        assert!(!check_file(base, cand, &none, &none, &none).is_empty());
    }

    #[test]
    fn tokens() {
        assert!(!errs(&with("", "g = unsafePerformIO x\n")).is_empty());
        assert!(!errs(&with("", "g = UNSAFEcoerce x\n")).is_empty());
        assert!(!errs(&with("", "g = runRW# x\n")).is_empty());
        // One more use of an identifier the baseline already uses once.
        assert!(!errs(&with("", "g = unsafeDeserialize y\n")).is_empty());
        assert!(!errs(&with("", "foreign import ccall \"getpid\" p :: Int\n")).is_empty());
        // In comments and strings it's fine.
        assert!(errs(&with("", "-- unsafePerformIO\ng = \"unsafePerformIO\"\n{- runRW# {- nested -} foreign import -}\n")).is_empty());
    }

    #[test]
    fn string_gaps_and_escapes_dont_desync() {
        // GHC ends each string at the last quote; code after it is code.
        for src in [
            "s = \"a\\   \\\"\nimport GHC.Exts\n",
            "s = \"a\\\n   \\\"\nimport GHC.Exts\n",
            "s = \"\\^\\\"\nimport GHC.Exts\n",
            "s = \"\\\\\"\nimport GHC.Exts\n",
            "c = '\\''\nimport GHC.Exts\n",
            "c = '\\^\\'\nimport GHC.Exts\n",
        ] {
            let l = lex(src);
            assert!(!l.malformed, "{src:?}");
            let ms: Vec<String> = imports(&l).into_iter().map(|(m, _)| m).collect();
            assert_eq!(ms, ["GHC.Exts"], "{src:?}");
        }
        for bad in [
            "s = \"abc\n",
            "{- open\n",
            "{-# OPTIONS_GHC\n",
            "s = \"a\\  x\"\n",
            "s = \"a\nb\"\n",
        ] {
            assert!(lex(bad).malformed, "{bad:?}");
            assert!(!errs(&with("", bad)).is_empty(), "{bad:?}");
        }
    }

    #[test]
    fn lexer() {
        let l = lex("a --> b -- c\nx = 'a' : y' : '\\n' : z''\n{- {-# not a pragma #-} -}\ns = \"{-# ANN #-}\"\n");
        assert!(l.pragmas.is_empty(), "{:?}", l.pragmas);
        assert!(l.code.contains("a --> b"));
        assert!(!l.code.contains(" c"));
        assert!(l.code.contains("y'"));
        assert_eq!(l.code.lines().count(), 4);
    }

    /// [haskell] src from the real challenge.toml.
    fn ledger_srcs() -> Vec<String> {
        let ch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../challenges/ledger-cbor");
        crate::challenge::Spec::load(&ch)
            .unwrap()
            .haskell
            .unwrap()
            .src
    }

    #[test]
    fn ledger_packages_txt_matches_challenge_toml() {
        let ch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../challenges/ledger-cbor");
        let Ok(txt) = std::fs::read_to_string(ch.join("packages.txt")) else {
            return;
        };
        let from_txt: Vec<String> = txt
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| format!("{}/src", l.split_whitespace().next().unwrap()))
            .collect();
        assert_eq!(from_txt, ledger_srcs());
    }

    #[test]
    fn real_baseline_is_clean() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../challenges/ledger-cbor/baseline");
        if dir.is_dir() {
            assert_eq!(check(&dir, &dir, &ledger_srcs()), Ok(()));
        }
    }

    /// The real ledger-cbor tree: broken variants are clean (they must reach
    /// the verifier), and issue #51's exploits on a real file are refused.
    #[test]
    fn real_variants_and_exploits() {
        let ch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../challenges/ledger-cbor");
        if !ch.is_dir() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("cb-hs-{}", std::process::id()));
        let fresh = |overlay: Option<&Path>| {
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).unwrap();
            for from in std::iter::once(ch.join("baseline")).chain(overlay.map(Path::to_path_buf)) {
                assert!(std::process::Command::new("cp")
                    .arg("-R")
                    .arg(from.join("."))
                    .arg(&tmp)
                    .status()
                    .unwrap()
                    .success());
            }
        };
        for v in std::fs::read_dir(ch.join("preflight/broken"))
            .unwrap()
            .flatten()
        {
            fresh(Some(&v.path()));
            assert_eq!(
                check(&ch.join("baseline"), &tmp, &ledger_srcs()),
                Ok(()),
                "{:?}",
                v.path()
            );
        }
        let file = tmp.join("cardano-ledger-binary/src/Cardano/Ledger/Binary/Decoding/Sized.hs");
        for (head, body) in [
            ("{-# options_ghc -F -pgmF ./pp.sh #-}\n", ""),
            ("{-#ANN module () #-}\n", ""),
            (
                "{-# options_ghc -cpp #-}\n",
                "\n# include \"/etc/passwd\"\n",
            ),
            (
                "{-# language MagicHash, UnboxedTuples #-}\n",
                "\nimport GHC.Exts\nimport GHC.ST\n",
            ),
        ] {
            fresh(None);
            let src = std::fs::read_to_string(&file).unwrap();
            let src = src.replacen("\nimport ", &format!("{body}\nimport "), 1);
            std::fs::write(&file, format!("{head}{src}")).unwrap();
            assert!(
                check(&ch.join("baseline"), &tmp, &ledger_srcs()).is_err(),
                "{head}{body}"
            );
        }
        // Outside src/, and new files.
        fresh(None);
        std::fs::write(tmp.join("cardano-ledger-core/x.cabal"), "x").unwrap();
        std::fs::write(
            tmp.join("cardano-ledger-binary/src/New.hs"),
            "module New where\n",
        )
        .unwrap();
        assert_eq!(
            check(&ch.join("baseline"), &tmp, &ledger_srcs())
                .unwrap_err()
                .len(),
            2
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
