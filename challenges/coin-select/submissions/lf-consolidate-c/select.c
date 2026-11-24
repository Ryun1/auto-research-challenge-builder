/* lf-consolidate-c: lf-consolidate (largest-first, one change output, plus
 * opportunistic consolidation of up to MAX_EXTRA UTxOs under DUST) in C, with
 * static buffers and no heap use, to keep peak memory low.
 * Replies are byte-identical to the Python lf-consolidate. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define DUST 2000000LL
#define MAX_EXTRA 2
#define MAXU 16384
#define IDLEN 32
#define LINE (1 << 20)

typedef long long i64;

static char line[LINE];
static char out[LINE];
static char ids[MAXU][IDLEN];
static i64 vals[MAXU];
static int nu;
static int order[MAXU];
static int picked[MAXU];
static int smalls[MAXU];

static i64 fee_a, fee_b, coins_per_byte, max_size, sz_base, sz_in, sz_out;

/* integer value following "key": anywhere after p (within the line) */
static i64 num_after(const char *p, const char *key) {
    const char *q = strstr(p, key);
    if (!q) return 0;
    q += strlen(key);
    while (*q == ' ' || *q == ':' || *q == '"') q++;
    return strtoll(q, NULL, 10);
}

/* copy the string value of "key": found inside [p, end) into dst */
static int str_in(const char *p, const char *end, const char *key, char *dst, int cap) {
    const char *q = strstr(p, key);
    if (!q || q >= end) return 0;
    q += strlen(key);
    while (*q == ' ' || *q == ':') q++;
    if (*q != '"') return 0;
    q++;
    int n = 0;
    while (*q && *q != '"' && n < cap - 1) dst[n++] = *q++;
    dst[n] = 0;
    return 1;
}

static i64 lov_in(const char *p, const char *end) {
    const char *q = strstr(p, "\"lovelace\"");
    if (!q || q >= end) return -1;
    q += 10;
    while (*q == ' ' || *q == ':') q++;
    return strtoll(q, NULL, 10);
}

static int by_value_desc(const void *a, const void *b) {
    int x = *(const int *)a, y = *(const int *)b;
    if (vals[x] != vals[y]) return vals[x] > vals[y] ? -1 : 1;
    return strcmp(ids[x], ids[y]);
}

static int by_value_asc(const void *a, const void *b) {
    int x = *(const int *)a, y = *(const int *)b;
    if (vals[x] != vals[y]) return vals[x] < vals[y] ? -1 : 1;
    return strcmp(ids[x], ids[y]);
}

static i64 size_of(int n_in, int n_out) { return sz_base + sz_in * n_in + sz_out * n_out; }
static i64 fee_of(int n_in, int n_out) { return fee_a * size_of(n_in, n_out) + fee_b; }

/* 1 = covered (change/fee set), 0 = not yet, -1 = too big */
static int finish(int n, i64 total, i64 need, int n_outs, i64 *change, i64 *fee) {
    i64 min_ada = (160 + sz_out) * coins_per_byte;
    if (size_of(n, n_outs + 1) > max_size) return -1;
    i64 spare = total - need, fc = fee_of(n, n_outs + 1);
    if (spare - fc >= min_ada) { *change = spare - fc; *fee = fc; return 1; }
    if (spare >= fee_of(n, n_outs)) { *change = -1; *fee = spare; return 1; }
    return 0;
}

static void add_utxo(const char *id, i64 v) {
    if (nu >= MAXU) exit(3);
    strncpy(ids[nu], id, IDLEN - 1);
    ids[nu][IDLEN - 1] = 0;
    vals[nu] = v;
    nu++;
}

static void deposit(const char *p) {
    const char *a = strstr(p, "\"utxos\"");
    if (!a) return;
    const char *o = strchr(a, '{');
    while (o) {
        const char *e = strchr(o, '}');
        if (!e) break;
        char id[IDLEN];
        i64 v = lov_in(o, e);
        if (str_in(o, e, "\"id\"", id, IDLEN) && v >= 0) add_utxo(id, v);
        o = strchr(e, '{');
    }
}

static void pay(const char *p) {
    char pid[IDLEN] = "";
    const char *a = strstr(p, "\"outputs\"");
    /* the pay id is the "id" key outside the outputs array */
    str_in(p, a ? a : p + strlen(p), "\"id\"", pid, IDLEN);
    if (!pid[0] && a) str_in(strchr(a, ']') ? strchr(a, ']') : a, p + strlen(p), "\"id\"", pid, IDLEN);
    i64 need = 0;
    int n_outs = 0;
    if (a) {
        const char *end = strchr(a, ']');
        const char *o = strchr(a, '{');
        while (o && end && o < end) {
            const char *e = strchr(o, '}');
            i64 v = lov_in(o, e);
            if (v >= 0) { need += v; n_outs++; }
            o = strchr(e, '{');
        }
    }
    for (int i = 0; i < nu; i++) order[i] = i;
    qsort(order, nu, sizeof(int), by_value_desc);
    int np = 0, ok = 0;
    i64 total = 0, change = -1, fee = 0;
    for (int k = 0; k < nu; k++) {
        picked[np++] = order[k];
        total += vals[order[k]];
        int r = finish(np, total, need, n_outs, &change, &fee);
        if (r < 0) break;
        if (r > 0) { ok = 1; break; }
    }
    if (!ok) { fputs("{\"refuse\":true}\n", stdout); fflush(stdout); return; }
    /* consolidation: smallest UTxOs under DUST worth more than their input fee */
    i64 in_fee = fee_a * sz_in;
    int ns = 0;
    for (int k = 0; k < nu; k++) {
        int chosen = 0;
        for (int j = 0; j < np; j++) if (picked[j] == k) { chosen = 1; break; }
        if (!chosen && vals[k] > in_fee && vals[k] < DUST) smalls[ns++] = k;
    }
    qsort(smalls, ns, sizeof(int), by_value_asc);
    for (int k = 0; k < ns && k < MAX_EXTRA; k++) {
        i64 c2, f2;
        if (finish(np + 1, total + vals[smalls[k]], need, n_outs, &c2, &f2) <= 0) break;
        picked[np++] = smalls[k];
        total += vals[smalls[k]];
        change = c2;
        fee = f2;
    }
    /* reply, in the same JSON shape Python's json.dumps writes */
    int w = snprintf(out, LINE, "{\"inputs\": [");
    for (int j = 0; j < np; j++)
        w += snprintf(out + w, LINE - w, "%s\"%s\"", j ? ", " : "", ids[picked[j]]);
    if (change >= 0) w += snprintf(out + w, LINE - w, "], \"change\": [%lld], \"fee\": %lld}\n", change, fee);
    else w += snprintf(out + w, LINE - w, "], \"change\": [], \"fee\": %lld}\n", fee);
    fputs(out, stdout);
    fflush(stdout);
    /* apply: drop the inputs (compact in place), then add the change output */
    for (int j = 0; j < np; j++) vals[picked[j]] = -1;
    int m = 0;
    for (int k = 0; k < nu; k++) {
        if (vals[k] < 0) continue;
        if (m != k) { memcpy(ids[m], ids[k], IDLEN); vals[m] = vals[k]; }
        m++;
    }
    nu = m;
    if (change >= 0) {
        char cid[IDLEN];
        snprintf(cid, IDLEN, "%s#0", pid);
        add_utxo(cid, change);
    }
}

int main(void) {
    while (fgets(line, LINE, stdin)) {
        const char *t = strstr(line, "\"type\"");
        if (!t) continue;
        t += 6;
        while (*t == ' ' || *t == ':') t++;
        if (!strncmp(t, "\"params\"", 8)) {
            fee_a = num_after(line, "\"min_fee_a\"");
            fee_b = num_after(line, "\"min_fee_b\"");
            coins_per_byte = num_after(line, "\"coins_per_utxo_byte\"");
            max_size = num_after(line, "\"max_tx_size\"");
            const char *s = strstr(line, "\"size\"");
            if (s) {
                sz_base = num_after(s, "\"base\"");
                sz_in = num_after(s, "\"input\"");
                sz_out = num_after(s, "\"output\"");
            }
        } else if (!strncmp(t, "\"deposit\"", 9)) {
            deposit(line);
        } else if (!strncmp(t, "\"pay\"", 5)) {
            pay(line);
        } else if (!strncmp(t, "\"end\"", 5)) {
            break;
        }
    }
    return 0;
}
