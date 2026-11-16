// Concurrent direct play (roadmap 11d): VUS clients each read one film in
// 4 MiB range requests, as a player does, for DURATION.
//   k6 run -e BASE=http://rustyfin-server -e ITEM=<movie id> -e VUS=10 stream.js
import http from 'k6/http';
import { check } from 'k6';

const BASE = __ENV.BASE || 'http://rustyfin-server';
const CHUNK = 4 * 1024 * 1024;

export const options = {
  scenarios: {
    play: { executor: 'constant-vus', vus: Number(__ENV.VUS || 10), duration: __ENV.DURATION || '60s' },
  },
  summaryTrendStats: ['avg', 'p(50)', 'p(95)', 'p(99)', 'max'],
  discardResponseBodies: true,
};

export function setup() {
  const res = http.post(`${BASE}/Users/AuthenticateByName`, JSON.stringify({ Username: 'stress', Pw: '' }), {
    headers: {
      'Content-Type': 'application/json',
      Authorization: 'MediaBrowser Client="k6", Device="k6", DeviceId="k6-play", Version="1"',
      'Tailscale-User-Login': 'stress@example.com',
      'Tailscale-User-Name': 'stress',
      'Tailscale-User-Groups': 'admin',
    },
    // Bodies are discarded by default here (streams); the login's is needed.
    responseType: 'text',
  });
  const token = res.json('AccessToken');
  const head = http.get(`${BASE}/Videos/${__ENV.ITEM}/stream?static=true&api_key=${token}`, {
    headers: { Range: 'bytes=0-0' },
  });
  const size = Number((head.headers['Content-Range'] || '/0').split('/')[1]);
  return { token, size };
}

export default function (d) {
  let offset = Math.floor(Math.random() * 8) * CHUNK;
  while (offset < d.size) {
    const end = Math.min(offset + CHUNK, d.size) - 1;
    const r = http.get(`${BASE}/Videos/${__ENV.ITEM}/stream?static=true&api_key=${d.token}`, {
      headers: { Range: `bytes=${offset}-${end}` },
      tags: { name: 'range 4MiB' },
    });
    check(r, { '206': (x) => x.status === 206 });
    offset = end + 1;
  }
}
