#!/usr/bin/env python3
"""Run the parity cases against Jellyfin and Rustyfin and report the gaps.

Jellyfin is the oracle: every case must pass against it (if one does not, the
recording is wrong, not the server). Rustyfin's failures are the parity gaps.

    python3 -I tests/parity/run.py                 # both servers, all areas
    python3 -I tests/parity/run.py library         # one area
    python3 -I tests/parity/run.py --write report.md
    python3 -I tests/parity/run.py --all           # every failing assert, not the first 12

Needs kubectl and the `hurl-runner`, `jellyfin` and `rustyfin-server` services in
the `PARITY_NAMESPACE` namespace (default `rustyfin-parity`), both servers bootstrapped (tests/parity/bootstrap/*.hurl).

Offline, against Rustyfin alone (the nightly CI run): the cases already hold
Jellyfin's recorded answers, so no Jellyfin is needed.

    python3 -I tests/parity/run.py --local --server rustyfin --strict
    # hurl on PATH; PARITY_RUSTYFIN_URL (default http://localhost:8096);
    # --strict exits 1 when Rustyfin has any gap.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

NS = os.environ.get("PARITY_NAMESPACE", "rustyfin-parity")
RUNNER = "hurl-runner"
KUBECTL = os.environ.get("KUBECTL", "kubectl")
ROOT = Path(__file__).resolve().parent

SERVERS = {
    "jellyfin": {
        "base_url": "http://jellyfin:8096",
        "login": (
            'Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="run-1", Version="1.0"\n'
        ),
        "body": '{ "Username": "parity", "Pw": "parity-pass" }',
    },
    "rustyfin": {
        "base_url": "http://rustyfin-server",
        "login": (
            'Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="run-1", Version="1.0"\n'
            "Tailscale-User-Login: parity@example.com\nTailscale-User-Name: parity\nTailscale-User-Groups: admin\n"
        ),
        "body": '{ "Username": "parity", "Pw": "" }',
    },
}


# --local: run hurl here instead of in the sandbox's runner pod.
LOCAL = False
WORK = "/tmp/parity"


def kubectl(*args, stdin=None):
    return subprocess.run([KUBECTL, "-n", NS, *args], input=stdin, capture_output=True, text=True)


def put_file(path, text):
    """Write a file where hurl will read it (here, or in the runner pod)."""
    if LOCAL:
        Path(path).write_text(text)
    else:
        kubectl("exec", "-i", RUNNER, "--", "sh", "-c", f"cat > {path}", stdin=text)


def hurl(file_in_runner, variables, *flags):
    args = ["hurl", "--json", "--no-color", *flags]
    for k, v in variables.items():
        args += ["--variable", f"{k}={v}"]
    args.append(file_in_runner)
    if LOCAL:
        run = subprocess.run(args, capture_output=True, text=True)
    else:
        run = kubectl("exec", RUNNER, "--", *args)
    try:
        return json.loads(run.stdout)
    except json.JSONDecodeError:
        sys.exit(f"hurl gave no JSON for {file_in_runner}:\n{run.stdout[:300]}\n{run.stderr[:300]}")


def login(server):
    cfg = SERVERS[server]
    text = (
        f"POST {{{{base_url}}}}/Users/AuthenticateByName\nContent-Type: application/json\n{cfg['login']}"
        f"{cfg['body']}\nHTTP 200\n[Captures]\ntoken: jsonpath \"$.AccessToken\"\n"
        'user_id: jsonpath "$.User.Id"\n'
    )
    put_file(f"{WORK}/login.hurl", text)
    res = hurl(f"{WORK}/login.hurl", {"base_url": cfg["base_url"]})
    caps = {c["name"]: c["value"] for c in res["entries"][0]["captures"]}
    return caps["token"], caps["user_id"]


def summarize(message):
    """One line for a failed assert: what was expected and what was seen."""
    lines = [l for l in message.splitlines() if l.strip()]
    head = lines[0] if lines else "assert failed"
    actual = next((l.strip(" |^") for l in lines if "actual" in l or "not found" in l or "invalid" in l), "")
    src = next((l.split("|", 1)[1].strip() for l in lines if l.lstrip()[:1].isdigit() and "|" in l), "")
    return f"{head}: {src} ({actual})" if src else f"{head} ({actual})"


def run_server(server, cases):
    token, user_id = login(server)
    base = SERVERS[server]["base_url"]
    results = {}
    for case in cases:
        res = hurl(
            f"{WORK}/cases/{case.name}",
            {"base_url": base, "token": token, "user_id": user_id},
            "--continue-on-error",
        )
        rows = []
        for e in res["entries"]:
            call = e["calls"][0]["request"] if e.get("calls") else {}
            failed = [a for a in e.get("asserts", []) if not a["success"]]
            rows.append(
                {
                    "line": e["line"],
                    "request": f"{call.get('method', '?')} {call.get('url', '?').replace(base, '')}",
                    "asserts": len(e.get("asserts", [])),
                    "failed": [summarize(a["message"]) for a in failed if "message" in a],
                    "no_response": not e.get("calls"),
                }
            )
        results[case.stem] = rows
    return results


def main():
    args = [a for a in sys.argv[1:]]
    out_file = None
    if "--write" in args:
        i = args.index("--write")
        out_file = args[i + 1]
        del args[i : i + 2]
    show_all = "--all" in args
    if show_all:
        args.remove("--all")
    global LOCAL, WORK
    LOCAL = "--local" in args
    if LOCAL:
        args.remove("--local")
        WORK = str(ROOT)
        SERVERS["rustyfin"]["base_url"] = os.environ.get(
            "PARITY_RUSTYFIN_URL", "http://localhost:8096"
        ).rstrip("/")
        SERVERS["jellyfin"]["base_url"] = os.environ.get(
            "PARITY_JELLYFIN_URL", "http://localhost:8097"
        ).rstrip("/")
    servers = list(SERVERS)
    if "--server" in args:
        i = args.index("--server")
        servers = [args[i + 1]]
        del args[i : i + 2]
    strict = "--strict" in args
    if strict:
        args.remove("--strict")
    only = args[0] if args else None

    cases = sorted((ROOT / "cases").glob("*.hurl"))
    cases = [c for c in cases if not only or c.stem == only]
    if not cases:
        sys.exit("no cases")

    if not LOCAL:
        kubectl("exec", RUNNER, "--", "rm", "-rf", WORK)
        cp = kubectl("cp", str(ROOT), f"{RUNNER}:{WORK}")
        if cp.returncode:
            sys.exit(f"cannot copy cases: {cp.stderr}")

    report = {s: run_server(s, cases) for s in servers}

    lines = ["# Parity report", ""]
    for server, per_case in report.items():
        total = sum(len(r) for r in per_case.values())
        bad = sum(1 for rows in per_case.values() for r in rows if r["failed"] or r["no_response"])
        lines.append(f"## {server}: {total - bad}/{total} requests match")
        for case, rows in per_case.items():
            for r in rows:
                if r["failed"] or r["no_response"]:
                    lines.append(f"- `{case}.hurl:{r['line']}` {r['request']}")
                    shown = r["failed"] if show_all else r["failed"][:12]
                    for f in shown:
                        lines.append(f"    - {f}")
                    if len(r["failed"]) > len(shown):
                        lines.append(f"    - ... {len(r['failed']) - len(shown)} more (--all shows them)")
        lines.append("")
    text = "\n".join(lines)
    print(text)
    if out_file:
        Path(out_file).write_text(text)

    # Jellyfin failing its own recording means the case is wrong.
    if "jellyfin" in report:
        jelly_bad = sum(1 for rows in report["jellyfin"].values() for r in rows if r["failed"])
        if jelly_bad:
            sys.exit(2)
    if strict and "rustyfin" in report:
        gaps = sum(
            1
            for rows in report["rustyfin"].values()
            for r in rows
            if r["failed"] or r["no_response"]
        )
        sys.exit(1 if gaps else 0)
    sys.exit(0)


if __name__ == "__main__":
    main()
