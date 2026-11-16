// /Items latency by result-set size (roadmap 11d), one scenario after another
// so each has its own numbers: a page of 100 from sets of ~300, ~3,000, ~75,000
// and every item (~80,000), sorted by name, plus a late page (StartIndex) and a
// search over everything. Run against a server holding make_library.sh's tree:
//   k6 run -e BASE=http://rustyfin-server -e VUS=5 items.js
import http from 'k6/http';
import { check } from 'k6';

const BASE = __ENV.BASE || 'http://rustyfin-server';
const VUS = Number(__ENV.VUS || 5);
const EACH = __ENV.EACH || '30s';

const QUERIES = {
  shows_300: 'IncludeItemTypes=Series&Recursive=true&SortBy=SortName&Limit=100',
  films_3k: 'IncludeItemTypes=Movie&Recursive=true&SortBy=SortName&Limit=100',
  albums_7k: 'IncludeItemTypes=MusicAlbum&Recursive=true&SortBy=SortName&Limit=100',
  tracks_75k: 'IncludeItemTypes=Audio&Recursive=true&SortBy=SortName&Limit=100',
  tracks_75k_late_page: 'IncludeItemTypes=Audio&Recursive=true&SortBy=SortName&Limit=100&StartIndex=70000',
  all_80k: 'Recursive=true&SortBy=SortName&Limit=100',
  search_all: 'Recursive=true&SearchTerm=track%2042&Limit=50',
  // Search hints go to Meilisearch when it is up (the Postgres fallback is
  // timed separately with EXPLAIN ANALYZE).
  hints: '__/Search/Hints?SearchTerm=film%20123&Limit=20',
};

const names = Object.keys(QUERIES);
const scenarios = {};
names.forEach((name, i) => {
  scenarios[name] = {
    executor: 'constant-vus',
    vus: VUS,
    duration: EACH,
    startTime: `${i * (parseInt(EACH, 10) + 2)}s`,
    env: { Q: QUERIES[name] },
    tags: { q: name },
  };
});

export const options = {
  scenarios,
  summaryTrendStats: ['avg', 'p(50)', 'p(95)', 'p(99)', 'max'],
  thresholds: Object.fromEntries(names.map((n) => [`http_req_duration{q:${n}}`, ['p(95)>=0']])),
};

export function setup() {
  const res = http.post(`${BASE}/Users/AuthenticateByName`, JSON.stringify({ Username: 'stress', Pw: '' }), {
    headers: {
      'Content-Type': 'application/json',
      Authorization: 'MediaBrowser Client="k6", Device="k6", DeviceId="k6-items", Version="1"',
      'Tailscale-User-Login': 'stress@example.com',
      'Tailscale-User-Name': 'stress',
      'Tailscale-User-Groups': 'admin',
    },
  });
  return { token: res.json('AccessToken') };
}

export default function (d) {
  const url = __ENV.Q.startsWith('__') ? `${BASE}${__ENV.Q.slice(2)}` : `${BASE}/Items?${__ENV.Q}`;
  const r = http.get(url, {
    headers: { Authorization: `MediaBrowser Token="${d.token}"` },
  });
  check(r, { 200: (x) => x.status === 200 });
}
