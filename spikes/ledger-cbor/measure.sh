#!/usr/bin/env bash
# measure.sh NAME CMD... : run CMD, write out/NAME.json with minutes, whole-system
# RAM delta, max process RSS and disk delta (same method as calibrate.yml).
set -uo pipefail
name=$1; shift
out=${OUT:?}; mkdir -p "$out"
before=$(df --output=used -B1 / | tail -1)
mem0=$(free -b | awk '/^Mem:/{print $3}')
(while sleep 2; do free -b | awk '/^Mem:/{print $3}'; done) > "$out/$name.mem" &
sampler=$!
start=$(date +%s)
/usr/bin/time -v -o "$out/$name.time" "$@" > "$out/$name.log" 2>&1
code=$?
end=$(date +%s)
kill $sampler
mem_peak=$(sort -n "$out/$name.mem" | tail -1)
after=$(df --output=used -B1 / | tail -1)
rss_kb=$(sed -n 's/.*Maximum resident set size (kbytes): //p' "$out/$name.time")
cat > "$out/$name.json" <<EOF
{"step": "$name", "exit": $code, "minutes": $(awk "BEGIN{printf \"%.1f\", ($end-$start)/60}"),
 "ram_gb": $(awk "BEGIN{printf \"%.2f\", (${mem_peak:-$mem0}-$mem0)/1e9}"),
 "max_process_rss_gb": $(awk "BEGIN{printf \"%.2f\", ${rss_kb:-0}/1048576}"),
 "disk_gb": $(awk "BEGIN{printf \"%.2f\", ($after-$before)/1e9}"),
 "disk_free_gb": $(df --output=avail -B1 / | tail -1 | awk '{printf "%.1f", $1/1e9}')}
EOF
cat "$out/$name.json"; tail -30 "$out/$name.log"
exit $code
