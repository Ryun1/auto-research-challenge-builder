//! `verifier/run`: bit-exact differential check. Prints `{"ok", "detail"}`.
fn main() {
    let seed = harness::seed_from_env();
    let n = harness::env_or("CB_VERIFY_N", harness::iface::VERIFY_N);
    match harness::verify(seed, n) {
        Ok(checked) => {
            println!("{{\"ok\": true, \"detail\": \"{checked} inputs bit-exact vs every oracle\"}}")
        }
        Err(e) => {
            println!("{{\"ok\": false, \"detail\": {:?}}}", e);
            std::process::exit(1);
        }
    }
}
