#!/usr/bin/env python3
"""Record Jellyfin's answers and write the Hurl case files that Rustyfin must match.

Jellyfin is the oracle. For each request in REQUESTS this asks the real Jellyfin
(through a pod inside the cluster namespace, because the namespace fence blocks the
caller from reaching sandbox pods) and writes a Hurl entry asserting what came back:
the status, the Content-Type, and the JSON *shape* (each key present and of the same
kind). Values that legitimately differ (ids, times, names, counts) are not asserted.

The generated files run unchanged against either server, so a case that fails on
Rustyfin is a parity gap. Run with -I (isolated mode):

    python3 -I tests/parity/gen/record.py            # all areas
    python3 -I tests/parity/gen/record.py system     # one area

Needs: kubectl, the `hurl-runner` and `jellyfin` pods in `hermes-sandbox`, and a
bootstrapped Jellyfin (tests/parity/bootstrap/jellyfin.hurl).
"""
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

NS = "hermes-sandbox"
RUNNER = "hurl-runner"
JELLYFIN = "http://jellyfin:8096"
KUBECTL = os.environ.get("KUBECTL", "kubectl")
OUT = Path(__file__).resolve().parent.parent / "cases"

AUTH = 'MediaBrowser Token="{{token}}"'

# Looked up per server at the top of every case file, so ids never leak into the cases.
PREAMBLE = f"""GET {{{{base_url}}}}/Items
Authorization: {AUTH}
[QueryStringParams]
Recursive: true
IncludeItemTypes: Movie
HTTP 200
[Captures]
movie_id: jsonpath "$.Items[0].Id"

GET {{{{base_url}}}}/Items
Authorization: {AUTH}
[QueryStringParams]
Recursive: true
IncludeItemTypes: Series
HTTP 200
[Captures]
series_id: jsonpath "$.Items[0].Id"

"""

# (area, method, path, query, needs_auth, json_body)
REQUESTS = [
    ("system", "GET", "/System/Info/Public", {}, False, None),
    ("system", "GET", "/System/Info", {}, True, None),
    ("system", "GET", "/System/Ping", {}, False, None),
    ("system", "GET", "/GetUtcTime", {}, False, None),
    ("system", "GET", "/Branding/Configuration", {}, False, None),
    ("system", "GET", "/QuickConnect/Enabled", {}, False, None),
    ("users", "GET", "/Users/Me", {}, True, None),
    ("users", "GET", "/Users", {}, True, None),
    ("users", "GET", "/Users/{user_id}", {}, True, None),
    ("library", "GET", "/Library/VirtualFolders", {}, True, None),
    ("library", "GET", "/Library/MediaFolders", {}, True, None),
    ("library", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Movie"}, True, None),
    ("library", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Series"}, True, None),
    ("library", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Episode"}, True, None),
    ("library", "GET", "/Items/{movie_id}", {}, True, None),
    ("library", "GET", "/Users/{user_id}/Items/{movie_id}", {}, True, None),
    ("library", "GET", "/Users/{user_id}/Views", {}, True, None),
    ("library", "GET", "/Items/Counts", {}, True, None),
    ("library", "GET", "/Items/Filters", {}, True, None),
    ("library", "GET", "/Genres", {}, True, None),
    ("library", "GET", "/Artists", {}, True, None),
    ("library", "GET", "/Shows/{series_id}/Seasons", {}, True, None),
    ("library", "GET", "/Shows/{series_id}/Episodes", {}, True, None),
    ("library", "GET", "/Users/{user_id}/Items/Latest", {}, True, None),
    ("playback", "POST", "/Items/{movie_id}/PlaybackInfo", {"UserId": "{user_id}"}, True, {}),
    ("sessions", "GET", "/Sessions", {}, True, None),
]


def kubectl(*args, stdin=None):
    return subprocess.run(
        [KUBECTL, "-n", NS, *args], input=stdin, capture_output=True, text=True, check=False
    )


def hurl_json(hurl_text, variables):
    """Run a Hurl file in the runner pod and return the parsed `--json` result."""
    put = kubectl("exec", "-i", RUNNER, "--", "sh", "-c", "cat > /tmp/rec.hurl", stdin=hurl_text)
    if put.returncode:
        sys.exit(f"cannot write to runner: {put.stderr}")
    args = ["exec", RUNNER, "--", "hurl", "--json", "--no-color"]
    for k, v in variables.items():
        args += ["--variable", f"{k}={v}"]
    args.append("/tmp/rec.hurl")
    run = kubectl(*args)
    try:
        return json.loads(run.stdout)
    except json.JSONDecodeError:
        sys.exit(f"hurl gave no JSON:\n{run.stdout[:400]}\n{run.stderr[:400]}")


def login():
    text = f"""POST {{{{base_url}}}}/Users/AuthenticateByName
Content-Type: application/json
Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="rec-1", Version="1.0"
{{ "Username": "parity", "Pw": "parity-pass" }}
HTTP 200
[Captures]
token: jsonpath "$.AccessToken"
user_id: jsonpath "$.User.Id"
"""
    res = hurl_json(text, {"base_url": JELLYFIN})
    caps = {c["name"]: c["value"] for c in res["entries"][0]["captures"]}
    return caps["token"], caps["user_id"]


def lookup_ids(token):
    res = hurl_json(PREAMBLE, {"base_url": JELLYFIN, "token": token})
    ids = {}
    for entry in res["entries"]:
        for c in entry.get("captures", []):
            ids[c["name"]] = c["value"]
    return ids


def kind(value):
    if isinstance(value, bool):
        return "isBoolean"
    if isinstance(value, int):
        return "isInteger"
    if isinstance(value, float):
        return "isNumber"
    if isinstance(value, str):
        return "isString"
    return "exists"


def shape(node, path, out, depth=0):
    """Assertions for each key of a JSON value: present, and the same kind."""
    if isinstance(node, dict):
        for key, val in node.items():
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
                continue  # keys like "en-US" need bracket syntax; not worth the noise
            if re.fullmatch(r"[0-9a-f]{16,}", key):
                continue  # a content hash (ImageBlurHashes.Primary.<hash>), differs per server
            child = f"{path}.{key}"
            if isinstance(val, list):
                out.append(f'jsonpath "{child}" isCollection')
                if val and depth < 3:
                    shape(val[0], f"{child}[0]", out, depth + 1)
            elif isinstance(val, dict):
                out.append(f'jsonpath "{child}" exists')
                if depth < 3:
                    shape(val, child, out, depth + 1)
            elif val is None:
                # Jellyfin sent null: there is no type to hold Rustyfin to, and a
                # server that omits the key says the same thing as one sending null.
                continue
            else:
                out.append(f'jsonpath "{child}" {kind(val)}')
    elif isinstance(node, list):
        out.append(f'jsonpath "{path}" isCollection')


def fetch(method, path, query, auth, body, token, variables):
    """Ask Jellyfin; return (status, content_type, parsed_json_or_None)."""
    q = "".join(
        f"{k}: {v}\n" for k, v in query.items()
    )
    text = f"{method} {{{{base_url}}}}{path}\n"
    if auth:
        text += f"Authorization: {AUTH}\n"
    if body is not None:
        text += "Content-Type: application/json\n"
    if q:
        text += "[QueryStringParams]\n" + q
    if body is not None:
        text += json.dumps(body) + "\n"
    text += "HTTP *\n[Captures]\nstatus: status\nctype: header \"Content-Type\"\nbody: body\n"
    res = hurl_json(text, {"base_url": JELLYFIN, "token": token, **variables})
    caps = {c["name"]: c["value"] for c in res["entries"][0]["captures"]}
    parsed = None
    try:
        parsed = json.loads(caps["body"])
    except (json.JSONDecodeError, TypeError):
        pass
    return int(caps["status"]), caps.get("ctype") or "", parsed


def entry(method, path, query, auth, body, status, ctype, parsed):
    text = f"{method} {{{{base_url}}}}{path}\n"
    if auth:
        text += f"Authorization: {AUTH}\n"
    if body is not None:
        text += "Content-Type: application/json\n"
    if query:
        text += "[QueryStringParams]\n" + "".join(f"{k}: {v}\n" for k, v in query.items())
    if body is not None:
        text += json.dumps(body) + "\n"
    text += f"HTTP {status}\n[Asserts]\n"
    asserts = []
    if ctype:
        asserts.append(f'header "Content-Type" contains "{ctype.split(";")[0].strip()}"')
    if parsed is not None:
        shape(parsed, "$", asserts)
    if not asserts:
        return text[: -len("[Asserts]\n")]
    return text + "\n".join(asserts) + "\n"


def main():
    only = sys.argv[1] if len(sys.argv) > 1 else None
    token, user_id = login()
    variables = {"user_id": user_id, **lookup_ids(token)}
    print("ids:", {k: v[:8] for k, v in variables.items()}, file=sys.stderr)

    by_area = {}
    for area, method, path, query, auth, body in REQUESTS:
        if only and area != only:
            continue
        sub = {k: v.format(**variables) if isinstance(v, str) else v for k, v in query.items()}
        real_path = path.format(**variables)
        status, ctype, parsed = fetch(method, real_path, sub, auth, body, token, variables)
        # The emitted file keeps {{placeholders}}, resolved per server by the preamble.
        emitted_path = re.sub(r"\{(\w+)\}", r"{{\1}}", path)
        emitted_query = {k: re.sub(r"\{(\w+)\}", r"{{\1}}", v) if isinstance(v, str) else v for k, v in query.items()}
        by_area.setdefault(area, []).append(
            entry(method, emitted_path, emitted_query, auth, body, status, ctype, parsed)
        )
        print(f"  {area:9} {method:4} {real_path[:60]:60} -> {status}", file=sys.stderr)

    OUT.mkdir(parents=True, exist_ok=True)
    for area, entries in by_area.items():
        header = (
            f"# Parity: {area}. Generated by tests/parity/gen/record.py from a real Jellyfin;\n"
            "# do not edit by hand, edit REQUESTS and regenerate.\n"
            "# Variables: base_url, token, user_id.\n\n"
        )
        (OUT / f"{area}.hurl").write_text(header + PREAMBLE + "\n".join(entries))
        print(f"wrote {OUT / f'{area}.hurl'} ({len(entries)} requests)", file=sys.stderr)


if __name__ == "__main__":
    main()
