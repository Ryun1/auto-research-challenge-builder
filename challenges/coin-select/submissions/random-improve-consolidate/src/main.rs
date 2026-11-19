#![allow(dead_code)]
use std::io::{self, BufRead, Write};

// ---------------------------------------------------------------- JSON
#[derive(Debug, Clone)]
enum J {
    Null,
    Bool(bool),
    Num(f64, Option<i64>),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, k: &str) -> Option<&J> {
        if let J::Obj(v) = self {
            v.iter().find(|(n, _)| n == k).map(|(_, x)| x)
        } else {
            None
        }
    }
    fn int(&self) -> Option<u64> {
        match self {
            J::Num(_, Some(i)) if *i >= 0 => Some(*i as u64),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        if let J::Str(s) = self { Some(s) } else { None }
    }
    fn arr(&self) -> Option<&Vec<J>> {
        if let J::Arr(a) = self { Some(a) } else { None }
    }
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn value(&mut self, depth: u32) -> Option<J> {
        if depth > 64 {
            return None;
        }
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Some(J::Obj(v));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    let x = self.value(depth + 1)?;
                    v.push((k, x));
                    self.ws();
                    match self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return Some(J::Obj(v));
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Some(J::Arr(v));
                }
                loop {
                    v.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return Some(J::Arr(v));
                        }
                        _ => return None,
                    }
                }
            }
            b'"' => Some(J::Str(self.string()?)),
            b't' => self.lit("true", J::Bool(true)),
            b'f' => self.lit("false", J::Bool(false)),
            b'n' => self.lit("null", J::Null),
            _ => self.number(),
        }
    }
    fn lit(&mut self, s: &str, v: J) -> Option<J> {
        if self.b[self.i..].starts_with(s.as_bytes()) {
            self.i += s.len();
            Some(v)
        } else {
            None
        }
    }
    fn number(&mut self) -> Option<J> {
        let st = self.i;
        let mut is_int = true;
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => is_int = false,
                _ => break,
            }
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[st..self.i]).ok()?;
        let f: f64 = s.parse().ok()?;
        let i = if is_int { s.parse::<i64>().ok() } else { None };
        Some(J::Num(f, i))
    }
    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'/' | b'\\' | b'"' => out.push(e),
                        b'u' => {
                            let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            let cp = u32::from_str_radix(h, 16).ok()?;
                            self.i += 4;
                            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return None,
                    }
                }
                _ => out.push(c),
            }
        }
    }
}

fn parse(line: &str) -> Option<J> {
    let mut p = P { b: line.as_bytes(), i: 0 };
    let v = p.value(0)?;
    Some(v)
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

// ---------------------------------------------------------------- PRNG
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------- selection
const CONSOLIDATE: bool = true;
const DUST: u64 = 2_000_000;
const MAX_EXTRA: usize = 3;

struct Params {
    a: u64,
    b: u64,
    max_size: u64,
    base: u64,
    inp: u64,
    outp: u64,
    min_ada: u64,
}

impl Params {
    fn size(&self, n_in: usize, n_out: usize) -> u64 {
        self.base + self.inp * n_in as u64 + self.outp * n_out as u64
    }
    fn fee(&self, n_in: usize, n_out: usize) -> u64 {
        self.a * self.size(n_in, n_out) + self.b
    }
}

struct Reply {
    inputs: Vec<usize>, // indexes into wallet
    change: Vec<u64>,
    fee: u64,
}

/// Build change for a selection with `total` input value. None if infeasible.
fn make_change(p: &Params, total: u64, n_in: usize, outs: &[u64]) -> Option<(Vec<u64>, u64)> {
    let need: u64 = outs.iter().sum();
    let n_out = outs.len();
    if p.size(n_in, n_out) > p.max_size {
        return None;
    }
    // payment-shaped: try k change outputs (k <= n_out), most first
    for k in (1..=n_out).rev() {
        if p.size(n_in, n_out + k) > p.max_size {
            continue;
        }
        let fee = p.fee(n_in, n_out + k);
        if total < need + fee {
            continue;
        }
        let left = total - need - fee;
        let shaped: u64 = outs[..k - 1].iter().sum();
        if left >= shaped + p.min_ada {
            let mut ch: Vec<u64> = outs[..k - 1].to_vec();
            ch.push(left - shaped);
            return Some((ch, fee));
        }
    }
    // no change: leftover goes to the fee
    let fee = p.fee(n_in, n_out);
    if total >= need + fee {
        return Some((Vec::new(), total - need));
    }
    None
}

fn largest_first(p: &Params, w: &[(String, u64)], outs: &[u64]) -> Option<Reply> {
    let mut idx: Vec<usize> = (0..w.len()).collect();
    idx.sort_by(|&x, &y| w[y].1.cmp(&w[x].1).then(w[x].0.cmp(&w[y].0)));
    let mut total = 0;
    let mut sel = Vec::new();
    for i in idx {
        sel.push(i);
        total += w[i].1;
        if p.size(sel.len(), outs.len()) > p.max_size {
            return None;
        }
        if let Some((change, fee)) = make_change(p, total, sel.len(), outs) {
            return Some(Reply { inputs: sel, change, fee });
        }
    }
    None
}

fn select(p: &Params, rng: &mut Rng, w: &[(String, u64)], outs: &[u64]) -> Option<Reply> {
    let need: u64 = outs.iter().sum();
    let n_out = outs.len();
    let mut pool: Vec<usize> = (0..w.len()).collect();
    let mut sel: Vec<usize> = Vec::new();
    let mut total = 0u64;
    let mut covered = false;

    // phase 1: random selection until covered (with a valid change output)
    while !pool.is_empty() {
        let k = rng.below(pool.len());
        let i = pool.swap_remove(k);
        sel.push(i);
        total += w[i].1;
        if p.size(sel.len(), n_out + 1) > p.max_size {
            break;
        }
        let fee = p.fee(sel.len(), n_out + 1);
        if total >= need + fee && total - need - fee >= p.min_ada {
            covered = true;
            break;
        }
    }
    if !covered {
        return largest_first(p, w, outs);
    }

    // phase 2: improve towards 2x the payment, never past 3x
    let ideal = need.saturating_mul(2);
    let maxt = need.saturating_mul(3);
    let mut tries = 0;
    while total < ideal && !pool.is_empty() && tries < 2 * n_out + 4 {
        tries += 1;
        let k = rng.below(pool.len());
        let i = pool[k];
        let nt = total + w[i].1;
        let dist = |t: u64| if t > ideal { t - ideal } else { ideal - t };
        if nt <= maxt && dist(nt) < dist(total) && p.size(sel.len() + 1, n_out + n_out) <= p.max_size {
            pool.swap_remove(k);
            sel.push(i);
            total = nt;
        }
    }

    // phase 3 (consolidate variant): add up to 3 of the smallest dust UTxOs
    if CONSOLIDATE {
        let mut small: Vec<usize> = pool.iter().copied().filter(|&i| w[i].1 < DUST).collect();
        small.sort_by(|&x, &y| w[x].1.cmp(&w[y].1).then(w[x].0.cmp(&w[y].0)));
        let extra_fee = p.a * p.inp;
        for &i in small.iter().take(MAX_EXTRA) {
            if w[i].1 <= extra_fee {
                continue; // costs more than it is worth
            }
            if p.size(sel.len() + 1, n_out + n_out) > p.max_size {
                break;
            }
            sel.push(i);
            total += w[i].1;
        }
    }

    match make_change(p, total, sel.len(), outs) {
        Some((change, fee)) => Some(Reply { inputs: sel, change, fee }),
        None => largest_first(p, w, outs),
    }
}

// ---------------------------------------------------------------- main
fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut params: Option<Params> = None;
    let mut wallet: Vec<(String, u64)> = Vec::new();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg = match parse(&line) {
            Some(m) => m,
            None => continue,
        };
        let ty = msg.get("type").and_then(|t| t.str()).unwrap_or("");
        match ty {
            "params" => {
                let g = |k: &str| msg.get(k).and_then(|x| x.int()).unwrap_or(0);
                let sz = msg.get("size");
                let gs = |k: &str| sz.and_then(|s| s.get(k)).and_then(|x| x.int()).unwrap_or(0);
                let outp = gs("output");
                params = Some(Params {
                    a: g("min_fee_a"),
                    b: g("min_fee_b"),
                    max_size: g("max_tx_size"),
                    base: gs("base"),
                    inp: gs("input"),
                    outp,
                    min_ada: (160 + outp) * g("coins_per_utxo_byte"),
                });
            }
            "deposit" => {
                if let Some(us) = msg.get("utxos").and_then(|u| u.arr()) {
                    for u in us {
                        if let (Some(id), Some(v)) =
                            (u.get("id").and_then(|x| x.str()), u.get("lovelace").and_then(|x| x.int()))
                        {
                            wallet.push((id.to_string(), v));
                        }
                    }
                }
            }
            "pay" => {
                let id = msg.get("id").and_then(|x| x.str()).unwrap_or("").to_string();
                let outs: Vec<u64> = msg
                    .get("outputs")
                    .and_then(|o| o.arr())
                    .map(|a| a.iter().filter_map(|o| o.get("lovelace").and_then(|x| x.int())).collect())
                    .unwrap_or_default();
                let reply = params.as_ref().and_then(|p| select(p, &mut rng, &wallet, &outs));
                let text = match reply {
                    None => "{\"refuse\":true}".to_string(),
                    Some(r) => {
                        let ids: Vec<String> = r.inputs.iter().map(|&i| esc(&wallet[i].0)).collect();
                        let ch: Vec<String> = r.change.iter().map(|c| c.to_string()).collect();
                        let text = format!(
                            "{{\"inputs\":[{}],\"change\":[{}],\"fee\":{}}}",
                            ids.join(","),
                            ch.join(","),
                            r.fee
                        );
                        let mut rm = r.inputs.clone();
                        rm.sort_unstable_by(|a, b| b.cmp(a));
                        for i in rm {
                            wallet.swap_remove(i);
                        }
                        for (k, c) in r.change.iter().enumerate() {
                            wallet.push((format!("{}#{}", id, k), *c));
                        }
                        text
                    }
                };
                let _ = writeln!(out, "{}", text);
                let _ = out.flush();
            }
            "end" => break,
            _ => {}
        }
    }
    let _ = out.flush();
}
