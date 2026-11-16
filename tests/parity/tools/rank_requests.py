"""Rank live client requests (request_log lines) and mark which the parity cases cover.

usage: python3 -I rank_requests.py <live.log> <tests/parity/cases dir>
"""
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

ID = re.compile(r"/[0-9a-fA-F]{8}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{12}(?=/|$)")
NUM = re.compile(r"/\d+(?=/|$)")
LINE = re.compile(r"request_log: (\w+) (\S+) -> (\d{3}) in ([\d.]+)(ms|µs|s)")


def shape(method, url):
    u = urlsplit(url)
    path = NUM.sub("/{n}", ID.sub("/{id}", u.path.replace("{{", "{").replace("}}", "}")))
    path = re.sub(r"\{[a-z_]+\}", "{id}", path).replace("{id}/{n}", "{id}/{n}")
    path = path.replace("/{n}", "/{n}")
    keys = sorted({k.lower() for k, _ in parse_qsl(u.query, keep_blank_values=True)} - {"api_key", "apikey", "tag"})
    return f"{method} {path}", keys


def ms(v, unit):
    return float(v) * {"ms": 1, "µs": 0.001, "s": 1000}[unit]


def main():
    log, cases = Path(sys.argv[1]), Path(sys.argv[2])
    hits, statuses, times, params = Counter(), defaultdict(Counter), defaultdict(list), defaultdict(Counter)
    for line in re.sub(r"\x1b\[[0-9;]*m", "", log.read_text(errors="replace")).splitlines():
        m = LINE.search(line)
        if not m:
            continue
        route, keys = shape(m[1], m[2])
        hits[route] += 1
        statuses[route][m[3]] += 1
        times[route].append(ms(m[4], m[5]))
        for k in keys:
            params[route][k] += 1
    covered = defaultdict(set)
    for f in cases.glob("*.hurl"):
        text = f.read_text()
        for block in re.split(r"\n(?=(?:GET|POST|DELETE|PUT) )", text):
            mm = re.match(r"(GET|POST|DELETE|PUT) \{\{base_url\}\}(\S+)", block)
            if not mm:
                continue
            route, keys = shape(mm[1], mm[2])
            q = re.search(r"\[QueryStringParams\]\n((?:[^\[\n][^\n]*\n)*)", block)
            if q:
                keys = set(keys) | {l.split(":", 1)[0].strip().lower() for l in q[1].splitlines()
                                    if ":" in l and not l.startswith(("HTTP", "#"))}
            covered[route].update(keys)
    total = sum(hits.values())
    print(f"{total} requests, {len(hits)} route shapes\n")
    for route, n in hits.most_common():
        t = sorted(times[route])
        p95 = t[min(len(t) - 1, int(len(t) * 0.95))]
        st = " ".join(f"{s}x{c}" for s, c in statuses[route].most_common())
        cov = "COVERED" if route in covered else "-------"
        missing = sorted(set(params[route]) - covered.get(route, set())) if route in covered else []
        print(f"{n:5} {cov} p95={p95:7.1f}ms [{st}] {route}")
        if params[route]:
            print("        params: " + ", ".join(f"{k}({c})" for k, c in params[route].most_common(14)))
        if missing:
            print("        params not in cases: " + ", ".join(missing))


main()
