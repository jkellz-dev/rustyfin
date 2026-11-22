#!/usr/bin/env python3
"""Catalog every HTTP route Jellyfin serves and mark what Rustyfin and the parity suite cover.

usage: python3 -I route_catalog.py <jellyfin source checkout> [<jellyfin checkout> ...]

Reads the `[Route]`/`[Http*]` attributes of each controller in
`Jellyfin.Api/Controllers` (several checkouts: the union, each route tagged with the
versions that serve it), the `.route(...)` calls in `crates/rustyfin-api/src`, and the
request lines of `tests/parity/cases/*.hurl`, then writes `tests/parity/ROUTES.md`.

A route is matched by method and path, with every `{param}` alike and in any case
(Rustyfin matches routes in any case). A Jellyfin route Rustyfin does not serve is
either outside a design boundary (`BOUNDARIES`, from the top of roadmap.md) or a gap.
"""
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
OUT = REPO / "tests/parity/ROUTES.md"

VERB = re.compile(r'\[Http(Get|Post|Delete|Put|Head|Patch)(?:\("([^"]*)"[^)]*\))?\]')
ROUTE = re.compile(r'\[Route\("([^"]*)"\)\]')
AUTH = re.compile(r'\[Authorize(?:\(Policy\s*=\s*Policies\.(\w+)\))?\]')
METHOD = re.compile(r"((?:\s*\[[^\n]*\]\s*\n)+)\s*public\s+(?:async\s+)?[\w<>\[\]?,\s]+?\s(\w+)\s*\(")

# Routes outside a design boundary (roadmap.md, "Design Boundaries"): (method regex,
# path regex over the normalised path, reason). First match wins.
BOUNDARIES = [
    (r".*", r"/livetv/.*", "Live TV, DVR and tuners"),
    (r".*", r"/plugins(/.*)?", "no plugins"),
    (r".*", r"/packages(/.*)?|/repositories", "no plugins (plugin repositories)"),
    (r".*", r"/web/configurationpages?", "no plugins (plugin configuration pages)"),
    (r".*", r"/channels(/.*)?", "no plugins (channels come from channel plugins)"),
    (r".*", r"/items/remotesearch/book", "no book libraries"),
    (r"POST|DELETE", r"/startup/.*", "configuration is a read-only file, not a setup wizard"),
    (r"POST", r"/system/configuration(/.*)?", "configuration is a read-only file"),
    (r"POST", r"/system/(restart|shutdown)", "the server's lifecycle belongs to its supervisor"),
    (r"POST", r"/system/mediaencoder/path", "configuration is a read-only file"),
    (r"POST|DELETE", r"/branding/splashscreen", "configuration is a read-only file (branding)"),
    (r"POST", r"/users/password|/users/forgotpassword(/pin)?|/users/{}/(easy)?password",
     "forward auth: Rustyfin never holds or checks a password"),
    (r"POST", r"/users", "forward auth: a user exists once the proxy first sends their identity"),
    (r"POST", r"/users/{}/authenticate", "forward auth: Rustyfin never checks a password"),
    (r"POST", r"/items/{}", "the server never writes to media (metadata edits are rustyfin-meta's)"),
    (r"POST", r"/items/{}/contenttype", "the server never writes to media"),
    (r"POST|DELETE", r"/items/{}/images/{}(/{})?(/index)?", "the server never writes to media (art is rustyfin-meta's)"),
    (r"POST", r"/items/{}/remoteimages/download", "the server never writes to media (art is rustyfin-meta's)"),
    (r"POST", r"/items/remotesearch/apply/{}", "the server never writes to media (metadata is rustyfin-meta's)"),
    (r"POST|DELETE", r"/audio/{}/lyrics", "the server never writes to media (lyrics files)"),
    (r"POST", r"/audio/{}/remotesearch/lyrics/{}", "the server never writes to media (lyrics files)"),
    (r"POST|DELETE", r"/videos/{}/subtitles(/{})?", "the server never writes to media (subtitle files)"),
    (r"DELETE", r"/items(/{})?", "the server never deletes media"),
]


def norm(path):
    p = "/" + path.strip("/")
    p = re.sub(r"\{[^}]+\}\.\{[^}]+\}", "{}", p)  # {segmentId}.{container}: one segment
    p = re.sub(r"\{[^}]+\}", "{}", p)
    return p.lower()


def jellyfin_routes(src):
    routes = []
    for f in sorted((src / "Jellyfin.Api/Controllers").glob("*.cs")):
        text = f.read_text(encoding="utf-8-sig")
        ctrl = f.stem.removesuffix("Controller")
        cls = re.search(r"public class (\w+)Controller", text)
        if not cls:
            continue
        head = text[: cls.start()]
        # BaseJellyfinApiController carries [Route("[controller]")]; a class [Route] replaces it.
        prefixes = [p.replace("[controller]", ctrl) for p in ROUTE.findall(head) or ["[controller]"]]
        cls_auth = AUTH.findall(head)
        for m in METHOD.finditer(text[cls.end():]):
            attrs, action = m.group(1), m.group(2)
            verbs = VERB.findall(attrs)
            if not verbs:
                continue
            if "AllowAnonymous" in attrs:
                policy = "anonymous"
            else:
                auth = AUTH.findall(attrs) or cls_auth
                policy = ",".join(a or "Default" for a in auth) if auth else "anonymous"
            for verb, tmpl in verbs:
                templates = [tmpl] if tmpl else (ROUTE.findall(attrs) or [""])
                for t in templates:
                    for pre in prefixes:
                        full = t if t.startswith("/") else "/".join(x for x in (pre.strip("/"), t.strip("/")) if x)
                        routes.append({
                            "method": verb.upper(),
                            "path": "/" + full.strip("/"),
                            "controller": ctrl,
                            "action": action,
                            "policy": policy,
                            "obsolete": "[Obsolete" in attrs,
                        })
    return routes


def rustyfin_routes():
    keys = set()
    for f in (REPO / "crates/rustyfin-api/src").rglob("*.rs"):
        text = f.read_text()
        for m in re.finditer(r'\.route\(\s*"([^"]+)",', text):
            # The method chain runs to the `)` closing `.route(`.
            depth, i = 1, m.end()
            while depth and i < len(text):
                depth += {"(": 1, ")": -1}.get(text[i], 0)
                i += 1
            path, chain = m.group(1), text[m.end():i]
            verbs = set(re.findall(r"\b(get|post|delete|put|head|patch)\(", chain))
            if "get" in verbs:
                verbs.add("head")  # axum answers HEAD for every GET route
            for verb in verbs:
                keys.add(f"{verb.upper()} {norm(path)}")
    return keys


def parity_cases():
    cases = defaultdict(set)
    for f in sorted((REPO / "tests/parity/cases").glob("*.hurl")):
        for line in f.read_text().splitlines():
            m = re.match(r"(GET|POST|DELETE|PUT|HEAD) \{\{base_url\}\}(/[^\s?]*)", line)
            if m:
                path = re.sub(r"\{\{[^}]+\}\}", "{x}", m.group(2))
                cases[f"{m.group(1)} {norm(path)}"].add(f.stem)
    # The preamble (id lookups) opens every file: not a case of its own.
    return cases


def boundary(method, key_path):
    for m_re, p_re, reason in BOUNDARIES:
        if re.fullmatch(m_re, method) and re.fullmatch(p_re, key_path):
            return reason
    return None


def version_of(src):
    out = subprocess.run(["git", "-C", str(src), "describe", "--tags", "--always"],
                         capture_output=True, text=True, check=False).stdout.strip()
    return out or src.name


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    sources = [Path(a) for a in sys.argv[1:]]
    versions = [version_of(s) for s in sources]
    merged = {}
    for src, ver in zip(sources, versions):
        for r in jellyfin_routes(src):
            key = f"{r['method']} {norm(r['path'])}"
            row = merged.setdefault(key, {**r, "key": key, "versions": []})
            if ver not in row["versions"]:
                row["versions"].append(ver)
    ours = rustyfin_routes()
    cases = parity_cases()
    preamble = {"GET /items", "GET /artists/albumartists", "GET /library/virtualfolders", "GET /shows/{}/seasons"}

    rows = sorted(merged.values(), key=lambda r: (r["controller"], r["key"].split(" ", 1)[1], r["method"]))
    counts = defaultdict(int)
    by_ctrl = defaultdict(list)
    for r in rows:
        method, path = r["key"].split(" ", 1)
        r["served"] = r["key"] in ours
        r["cases"] = sorted(cases.get(r["key"], ()))
        r["boundary"] = boundary(method, path)
        if r["boundary"] and not r["served"]:
            state = "boundary"
        elif r["served"]:
            state = "served"
        else:
            state = "missing"
        r["state"] = state
        counts[state] += 1
        counts["cased"] += bool(r["cases"])
        if r["served"] and not r["boundary"]:
            counts["served_uncased"] += not r["cases"]
        by_ctrl[r["controller"]].append(r)

    lines = [
        "# Jellyfin routes",
        "",
        "Generated by `tests/parity/tools/route_catalog.py` from Jellyfin "
        + ", ".join(f"`{v}`" for v in versions)
        + ";",
        "do not edit by hand, re-run it. Every route a Jellyfin controller declares, whether",
        "Rustyfin serves it, which parity case files ask it, and the design boundary that",
        "leaves it out, if one does (roadmap.md, \"Design Boundaries\").",
        "",
        f"- Jellyfin routes: {len(rows)}",
        f"- Served by Rustyfin: {counts['served']}",
        f"- Missing: {counts['missing']}",
        f"- Outside a design boundary: {counts['boundary']}",
        f"- Asked by a parity case: {counts['cased']}",
        f"- Served but asked by no case: {counts['served_uncased']}",
        "",
        "Status: **served**, **missing** (a gap), or **boundary** (left out on purpose).",
        "Cases: the `tests/parity/cases` files that ask the route; `-` means none does yet.",
        "A route marked obsolete is one Jellyfin keeps for old clients.",
        "",
    ]
    for ctrl in sorted(by_ctrl):
        lines += [f"## {ctrl}", "", "| Route | Status | Cases | Auth | Note |", "| --- | --- | --- | --- | --- |"]
        for r in by_ctrl[ctrl]:
            note = []
            if r["obsolete"]:
                note.append("obsolete")
            if len(r["versions"]) < len(versions):
                note.append("only " + ", ".join(r["versions"]))
            if r["boundary"]:
                note.append(r["boundary"])
            cased = ", ".join(r["cases"]) or ("preamble" if r["key"] in preamble else "-")
            lines.append(
                f"| `{r['method']} {r['path']}` | {r['state']} | {cased} | {r['policy']} | {'; '.join(note)} |"
            )
        lines.append("")
    OUT.write_text("\n".join(lines))
    print(
        f"{len(rows)} routes: {counts['served']} served, {counts['missing']} missing, "
        f"{counts['boundary']} boundary; {counts['cased']} with a case -> {OUT.relative_to(REPO)}"
    )


if __name__ == "__main__":
    main()
