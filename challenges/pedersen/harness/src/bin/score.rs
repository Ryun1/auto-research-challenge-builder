//! `bench/run`: instruction count ranks, paired wall time guards.
//! Prints `{"metrics": {...}}`.
use std::process::Command;

/// Instructions retired inside `cb_measured`, from callgrind's totals line.
fn instructions(workload: &str, side: &str, seed: u64, n: usize) -> Result<u64, String> {
    let out = std::env::temp_dir().join(format!("cb-callgrind-{}-{side}", std::process::id()));
    let status = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--toggle-collect=cb_measured",
            "--quiet",
        ])
        .arg(format!("--callgrind-out-file={}", out.display()))
        .args([workload, side, &seed.to_string(), &n.to_string()])
        .status()
        .map_err(|e| format!("valgrind: {e} (Linux only; on a Mac run bench/run-docker)"))?;
    if !status.success() {
        return Err(format!("valgrind exited {status}"));
    }
    let text = std::fs::read_to_string(&out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&out);
    text.lines()
        .find_map(|l| {
            l.strip_prefix("totals:")
                .or_else(|| l.strip_prefix("summary:"))
        })
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .ok_or_else(|| "no totals line in callgrind output".into())
}

fn main() {
    let seed = harness::seed_from_env();
    let n = harness::env_or("CB_SCORE_N", harness::iface::SCORE_N);
    let blocks = harness::env_or("CB_PERF_BLOCKS", 4usize);
    let max_dispersion = harness::env_or("CB_PERF_MAX_DISPERSION", 0.08f64);
    let workload = std::env::current_exe().unwrap().with_file_name("workload");
    let workload = workload.to_string_lossy();
    let counts = instructions(&workload, "candidate", seed, n)
        .and_then(|c| Ok((c, instructions(&workload, "reference", seed, n)?)));
    let (ic, ir) = match counts {
        Ok(v) => v,
        Err(e) => {
            eprintln!("score: {e}");
            std::process::exit(1);
        }
    };
    let g = harness::guard(&harness::inputs(seed, n), blocks);
    // Wall time may not regress beyond noise; an unstable run is an infra
    // failure, not a rejection (provably.fast core.py:7822).
    let stable = g.rel_mad_candidate <= max_dispersion
        && g.rel_mad_reference <= max_dispersion
        && g.cross_block <= max_dispersion;
    let guard = if !stable {
        "unstable"
    } else if g.speedup < 1.0 - max_dispersion {
        "regressed"
    } else {
        "pass"
    };
    println!(
        "{{\"metrics\": {{\"speedup\": {}, \"instructions\": {ic}, \"reference_instructions\": {ir}, \
         \"wall_speedup\": {:.4}, \"wall_rel_mad_candidate\": {:.4}, \"wall_rel_mad_reference\": {:.4}, \
         \"wall_cross_block\": {:.4}, \"guard\": \"{guard}\", \"n\": {n}}}}}",
        ir as f64 / ic as f64,
        g.speedup,
        g.rel_mad_candidate,
        g.rel_mad_reference,
        g.cross_block
    );
}
