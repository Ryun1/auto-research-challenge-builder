//! pallas on the judge's inputs, with the judge harness's interface:
//!
//!   pallas-decode decode FILE REPS       decode every tx REPS times (0 = read only)
//!   pallas-decode decode-hash FILE REPS  same, plus blake2b-256 of the body bytes (the tx id)
//!   pallas-decode check FILE             how many inputs pallas accepts
//!
//! FILE holds one hex-encoded transaction per line. Prints decode seconds.
//! `Tx<'b>` borrows from the input and keeps the body, witness and aux-data
//! bytes (`KeepRaw`), which is what the ledger's Annotator path is for.
use pallas_codec::minicbor;
use pallas_crypto::hash::Hasher;
use pallas_primitives::conway::Tx;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, path) = match args.as_slice() {
        [m, p, ..] => (m.as_str(), p),
        _ => panic!("usage: pallas-decode decode|decode-hash FILE REPS | check FILE"),
    };
    let raw = std::fs::read_to_string(path).expect("read FILE");
    let txs: Vec<Vec<u8>> = raw
        .lines()
        .map(|l| hex::decode(l.trim()).expect("hex"))
        .collect();

    if mode == "check" {
        let bad: Vec<usize> = (0..txs.len())
            .filter(|&i| minicbor::decode::<Tx>(&txs[i]).is_err())
            .collect();
        println!(
            "{} inputs, {} rejected {:?}",
            txs.len(),
            bad.len(),
            &bad[..bad.len().min(10)]
        );
        std::process::exit(if bad.is_empty() { 0 } else { 1 });
    }

    let hash = match mode {
        "decode" => false,
        "decode-hash" => true,
        _ => panic!("unknown mode {mode}"),
    };
    let reps: usize = args[2].parse().expect("REPS");
    let t0 = Instant::now();
    for _ in 0..reps {
        for b in &txs {
            let tx: Tx = minicbor::decode(black_box(b)).expect("pallas rejected a tx");
            if hash {
                black_box(Hasher::<256>::hash(tx.transaction_body.raw_cbor()));
            }
            black_box(&tx);
        }
    }
    println!("{}", t0.elapsed().as_secs_f64());
}
