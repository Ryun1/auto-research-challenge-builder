//! Run under callgrind by `score`: `workload <candidate|reference> <seed> <n>`.
//! Input generation is outside the measured region.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let side = args[1] == "candidate";
    let seed: u64 = args[2].parse().unwrap();
    let n: usize = args[3].parse().unwrap();
    let inputs = harness::inputs(seed, n);
    harness::cb_measured(side, &inputs);
}
