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

# Deliberate differences from Jellyfin, each an owner decision. An assert matching
# `assert_re` in a request matching `path` and `query` (a subset of the request's
# query; None: no query at all) is written as a comment naming the reason instead of an assert, so the
# difference stays visible in the case file without failing the run.
KNOWN_DIFFERENCES = [
    (
        "/Items",
        {"IncludeItemTypes": "Audio"},
        r'\.PremiereDate" ',
        "tracks carry only ProductionYear, not a full release date (owner decision 2026-11-14)",
    ),
    (
        "/Plugins",
        {},
        r'\$\[0\]',
        "no plugins: a design boundary (roadmap.md); Jellyfin lists its bundled ones",
    ),
    (
        "/Items/{movie_id}/Images",
        {},
        r'\$\[0\]',
        "Jellyfin grabs a video frame as the poster of a film without art; Rustyfin does not extract images",
    ),
    (
        "/ScheduledTasks",
        {},
        r'LastExecutionResult\.|Triggers\[0\]\.IntervalTicks',
        "task history and schedules differ per server; the fields are there when a task has run",
    ),
    (
        "/Users/{user_id}/Items/Latest",
        None,
        r'\$\[0\]\.(Container|RunTimeTicks|VideoType|Artists|ArtistItems|AlbumArtist)',
        "the synthetic titles share one date added, and the two servers break the tie differently (film first or show first)",
    ),
    (
        "/Sessions",
        {},
        r'RemoteEndPoint',
        "behind forward auth the client's address is the proxy's, so Rustyfin leaves it out",
    ),
]


def known_difference(path, query, assertion):
    """The reason `assertion` is a deliberate difference for this request, if it is one."""
    for kd_path, kd_query, assert_re, reason in KNOWN_DIFFERENCES:
        # `{movie_id}` in a known path stands for whatever id was looked up.
        kd_re = re.sub(r"\\\{\w+\\\}", "[^/]+", re.escape(kd_path))
        if (
            re.fullmatch(kd_re, path)
            and (not query if kd_query is None else all(query.get(k) == v for k, v in kd_query.items()))
            and re.search(assert_re, assertion)
        ):
            return reason
    return None
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

GET {{{{base_url}}}}/Items
Authorization: {AUTH}
[QueryStringParams]
Recursive: true
IncludeItemTypes: MusicAlbum
HTTP 200
[Captures]
album_id: jsonpath "$.Items[0].Id"

GET {{{{base_url}}}}/Artists/AlbumArtists
Authorization: {AUTH}
HTTP 200
[Captures]
artist_id: jsonpath "$.Items[0].Id"

GET {{{{base_url}}}}/Library/VirtualFolders
Authorization: {AUTH}
HTTP 200
[Captures]
movies_lib_id: jsonpath "$[?(@.CollectionType=='movies')].ItemId"
tv_lib_id: jsonpath "$[?(@.CollectionType=='tvshows')].ItemId"
music_lib_id: jsonpath "$[?(@.CollectionType=='music')].ItemId"

GET {{{{base_url}}}}/Shows/{{{{series_id}}}}/Seasons
Authorization: {AUTH}
HTTP 200
[Captures]
season_id: jsonpath "$.Items[0].Id"

GET {{{{base_url}}}}/Items
Authorization: {AUTH}
[QueryStringParams]
Recursive: true
IncludeItemTypes: Audio
SortBy: SortName
HTTP 200
[Captures]
track_id: jsonpath "$.Items[0].Id"
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
    # Second slice: discovery, browsing helpers, per-user lists, localisation.
    ("system", "GET", "/System/Endpoint", {}, True, None),
    ("system", "GET", "/ScheduledTasks", {}, True, None),
    ("system", "GET", "/Plugins", {}, True, None),
    ("system", "GET", "/Library/PhysicalPaths", {}, True, None),
    ("localization", "GET", "/Localization/Countries", {}, True, None),
    ("localization", "GET", "/Localization/Cultures", {}, True, None),
    ("localization", "GET", "/Localization/Options", {}, True, None),
    ("localization", "GET", "/Localization/ParentalRatings", {}, True, None),
    ("users", "GET", "/Users/{user_id}/GroupingOptions", {}, True, None),
    ("users", "GET", "/DisplayPreferences/usersettings", {"userId": "{user_id}", "client": "emby"}, True, None),
    ("browse", "GET", "/Studios", {}, True, None),
    ("browse", "GET", "/Persons", {}, True, None),
    ("browse", "GET", "/Search/Hints", {"searchTerm": "movie"}, True, None),
    ("browse", "GET", "/Shows/NextUp", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Shows/Upcoming", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/UserItems/Resume", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Users/{user_id}/Items/Resume", {}, True, None),
    ("browse", "GET", "/Movies/Recommendations", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Items/{movie_id}/Similar", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Items/{movie_id}/Ancestors", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Items/{movie_id}/ThemeMedia", {"UserId": "{user_id}"}, True, None),
    ("browse", "GET", "/Items/{series_id}", {}, True, None),
    ("browse", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Audio"}, True, None),
    ("browse", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "MusicAlbum"}, True, None),
    ("browse", "GET", "/Items", {"ParentId": "{series_id}"}, True, None),
    ("browse", "GET", "/Items", {"Recursive": "true", "SortBy": "SortName", "SortOrder": "Ascending", "Limit": "2"}, True, None),
    ("browse", "GET", "/Items", {"Recursive": "true", "Filters": "IsUnplayed", "IncludeItemTypes": "Movie"}, True, None),
    ("browse", "GET", "/Items", {"Recursive": "true", "Fields": "Overview,Genres,People,Studios,MediaSources", "IncludeItemTypes": "Movie"}, True, None),
    ("browse", "GET", "/Items/{movie_id}/Images", {}, True, None),
    # Third slice: what Plezy asks for (its music pages, home rows and Continue
    # Watching), each a bug found in the app first (2026-11-15).
    ("music", "GET", "/Artists/AlbumArtists", {"UserId": "{user_id}", "Fields": "RecursiveItemCount,ChildCount,SortName"}, True, None),
    ("music", "GET", "/Items", {"AlbumArtistIds": "{artist_id}", "IncludeItemTypes": "MusicAlbum", "Recursive": "true", "SortBy": "PremiereDate,ProductionYear,SortName", "SortOrder": "Descending"}, True, None),
    ("music", "GET", "/Items", {"AlbumIds": "{album_id}", "IncludeItemTypes": "Audio", "Recursive": "true", "SortBy": "ParentIndexNumber,IndexNumber,SortName", "SortOrder": "Ascending", "Fields": "UserData"}, True, None),
    ("music", "GET", "/Items", {"ArtistIds": "{artist_id}", "IncludeItemTypes": "Audio", "Recursive": "true"}, True, None),
    ("music", "GET", "/Items/{album_id}", {"UserId": "{user_id}"}, True, None),
    ("music", "GET", "/Users/{user_id}/Items/Latest", {"IncludeItemTypes": "Audio", "Limit": "20"}, True, None),
    ("music", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Audio", "Filters": "IsPlayed", "SortBy": "DatePlayed", "SortOrder": "Descending"}, True, None),
    ("music", "GET", "/Shows/NextUp", {"UserId": "{user_id}", "NextUpDateCutoff": "2025-11-15T00:00:00Z", "EnableResumable": "false"}, True, None),
    ("music", "GET", "/UserItems/Resume", {"UserId": "{user_id}", "MediaTypes": "Video"}, True, None),
    ("music", "GET", "/Items", {"Recursive": "true", "Filters": "IsResumable"}, True, None),
    # Extras (roadmap 11j): a trailer and a featurette beside the test film.
    ("extras", "GET", "/Items/{movie_id}/LocalTrailers", {"UserId": "{user_id}"}, True, None),
    ("extras", "GET", "/Items/{movie_id}/SpecialFeatures", {"UserId": "{user_id}"}, True, None),
    ("extras", "GET", "/Users/{user_id}/Items/{movie_id}", {}, True, None),
    ("extras", "GET", "/Items", {"Recursive": "true", "IncludeItemTypes": "Movie"}, True, None),
    # Fourth slice (roadmap 11a): the owner's Plezy session, 2026-11-16, exact queries.
    ("plezy", "GET", "/UserItems/Resume", {'UserId': '{user_id}', 'Limit': '21', 'Fields': 'Overview,DateCreated', 'MediaTypes': 'Video', 'Recursive': 'true', 'EnableTotalRecordCount': 'false', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Shows/NextUp", {'UserId': '{user_id}', 'Limit': '21', 'Fields': 'Overview,DateCreated', 'EnableResumable': 'false', 'NextUpDateCutoff': '2025-11-16T00:00:00Z', 'EnableTotalRecordCount': 'false', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Users/{user_id}/Items/Latest", {'Limit': '20', 'ParentId': '{movies_lib_id}', 'Fields': 'Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Users/{user_id}/Items/Latest", {'Limit': '20', 'ParentId': '{music_lib_id}', 'Fields': 'PremiereDate,OriginalTitle,SortName,DateCreated', 'EnableUserData': 'false', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'IncludeItemTypes': 'Playlist', 'Recursive': 'true', 'MediaTypes': 'Video', 'StartIndex': '0', 'Limit': '200', 'Fields': 'Overview,DateCreated,DateLastSaved,DateModified,ChildCount,Tags', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'ParentId': '{movies_lib_id}', 'Recursive': 'true', 'StartIndex': '0', 'Limit': '72', 'EnableTotalRecordCount': 'true', 'IncludeItemTypes': 'Movie', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'ParentId': '{movies_lib_id}', 'Recursive': 'true', 'StartIndex': '0', 'Limit': '72', 'EnableTotalRecordCount': 'true', 'IncludeItemTypes': 'Movie', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'NameStartsWith': 'T', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'ParentId': '{tv_lib_id}', 'Recursive': 'true', 'StartIndex': '0', 'Limit': '72', 'EnableTotalRecordCount': 'true', 'IncludeItemTypes': 'Series', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items/Filters", {'UserId': '{user_id}', 'ParentId': '{movies_lib_id}'}, True, None),
    ("plezy", "GET", "/Artists/AlbumArtists", {'UserId': '{user_id}', 'ParentId': '{music_lib_id}', 'StartIndex': '0', 'Limit': '72', 'EnableTotalRecordCount': 'true', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'IncludeItemTypes': 'BoxSet', 'Recursive': 'true', 'StartIndex': '0', 'Limit': '36', 'SortBy': 'SortName', 'SortOrder': 'Ascending', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'SearchTerm': 'test', 'Recursive': 'true', 'Limit': '100', 'IncludeItemTypes': 'Series,Episode', 'ParentId': '{tv_lib_id}', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableTotalRecordCount': 'false', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Artists", {'UserId': '{user_id}', 'SearchTerm': 'test', 'Limit': '100', 'ParentId': '{music_lib_id}', 'EnableTotalRecordCount': 'false', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Persons", {'UserId': '{user_id}', 'SearchTerm': 'test', 'PersonTypes': 'Actor,Director', 'Limit': '100', 'EnableImageTypes': 'Primary', 'ImageTypeLimit': '1', 'EnableUserData': 'false', 'EnableTotalRecordCount': 'false'}, True, None),
    ("plezy", "GET", "/Users/{user_id}/Items/{series_id}", {'Fields': 'Overview,Genres,People,Studios,ProductionLocations,Tags,Taglines,DateCreated,DateLastSaved,PremiereDate,RecursiveItemCount,ChildCount,UserData,MediaSources,OriginalTitle,SortName,Chapters,Trickplay,ProviderIds'}, True, None),
    ("plezy", "GET", "/Users/{user_id}/Items/{movie_id}", {'Fields': 'Overview,Genres,People,Studios,ProductionLocations,Tags,Taglines,DateCreated,DateLastSaved,PremiereDate,RecursiveItemCount,ChildCount,UserData,MediaSources,OriginalTitle,SortName,Chapters,Trickplay,ProviderIds'}, True, None),
    ("plezy", "GET", "/Shows/NextUp", {'SeriesId': '{series_id}', 'UserId': '{user_id}', 'Limit': '1', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated,MediaSources', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items/{movie_id}/LocalTrailers", {'UserId': '{user_id}', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items/{movie_id}/SpecialFeatures", {'UserId': '{user_id}', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Shows/{series_id}/Seasons", {'UserId': '{user_id}', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items/{series_id}/Similar", {'UserId': '{user_id}', 'Limit': '10', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Shows/{series_id}/Episodes", {'UserId': '{user_id}', 'SeasonId': '{season_id}', 'StartIndex': '0', 'Limit': '200', 'EnableTotalRecordCount': 'true', 'IsMissing': 'false', 'IsVirtualUnaired': 'false', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated,MediaSources', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Shows/{series_id}/Episodes", {'UserId': '{user_id}', 'Fields': 'UserData,PremiereDate', 'StartIndex': '0', 'Limit': '200', 'IsMissing': 'false', 'IsVirtualUnaired': 'false', 'SortBy': 'ParentIndexNumber,IndexNumber,SortName', 'SortOrder': 'Ascending,Ascending,Ascending', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'ParentId': '{season_id}', 'Fields': 'RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated,MediaSources', 'StartIndex': '0', 'Limit': '500', 'SortBy': 'ParentIndexNumber,IndexNumber,SortName', 'SortOrder': 'Ascending,Ascending,Ascending', 'EnableImageTypes': 'Primary,Backdrop,Logo', 'ImageTypeLimit': '3'}, True, None),
    ("plezy", "GET", "/Items", {'UserId': '{user_id}', 'ParentId': '{series_id}', 'IncludeItemTypes': 'Episode', 'Recursive': 'true', 'SortBy': 'DatePlayed', 'SortOrder': 'Descending', 'Fields': 'UserData', 'Limit': '1', 'EnableImages': 'false', 'EnableTotalRecordCount': 'false'}, True, None),
    ("plezy", "GET", "/Items", {'Ids': '{movie_id}', 'UserId': '{user_id}', 'Fields': 'CanDelete', 'EnableImages': 'false', 'EnableUserData': 'false', 'EnableTotalRecordCount': 'false'}, True, None),
    ("plezy", "POST", "/Items/{movie_id}/PlaybackInfo", {'UserId': '{user_id}', 'MediaSourceId': '{movie_id}'}, True, {}),
    ("plezy", "GET", "/MediaSegments/{movie_id}", {}, True, None),
    ("plezy", "GET", "/MediaSegments/{track_id}", {}, True, None),
    ("plezy", "GET", "/Audio/{track_id}/Lyrics", {}, True, None),
    ("plezy", "GET", "/Audio/{movie_id}/Lyrics", {}, True, None),
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
    if isinstance(value, (int, float)):
        # C# serialises a whole double as `24`, Rust an f64 as `24.0`: both are numbers.
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
        # A top-level list (LocalTrailers, Ancestors, /Users): hold its
        # first element to the same shape too, so an empty answer fails.
        if node and depth < 3:
            out.append(f'jsonpath "{path}[0]" exists')
            shape(node[0], f"{path}[0]", out, depth + 1)


def fetch(method, path, query, auth, body, token, variables):
    """Ask Jellyfin; return (status, content_type, parsed_json_or_None)."""
    q = "".join(f"{k}: {v}\n" for k, v in query.items())
    text = f"{method} {{{{base_url}}}}{path}\n"
    if auth:
        text += f"Authorization: {AUTH}\n"
    if body is not None:
        text += "Content-Type: application/json\n"
    if q:
        text += "[QueryStringParams]\n" + q
    if body is not None:
        text += json.dumps(body) + "\n"
    # Only captures that cannot fail: a header capture on a response without that
    # header aborts the entry and loses the body with it.
    text += "HTTP *\n[Captures]\nbody: body\n"
    res = hurl_json(text, {"base_url": JELLYFIN, "token": token, **variables})
    entry0 = res["entries"][0]
    caps = {c["name"]: c["value"] for c in entry0.get("captures", [])}
    response = entry0["calls"][-1]["response"]
    ctype = next(
        (h["value"] for h in response.get("headers", []) if h["name"].lower() == "content-type"),
        "",
    )
    parsed = None
    try:
        parsed = json.loads(caps["body"])
    except (json.JSONDecodeError, TypeError, KeyError):
        pass
    return int(response["status"]), ctype, parsed


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
    for i, assertion in enumerate(asserts):
        reason = known_difference(path, query, assertion)
        if reason:
            asserts[i] = f"# known difference, not asserted: {assertion} ({reason})"
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
