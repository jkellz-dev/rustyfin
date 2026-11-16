// Load test of the requests clients make most (roadmap 11d), shaped like
// Plezy's (from the owner's request log). Runs from a pod beside the server:
//   k6 run -e BASE=http://rustyfin-server -e VUS=20 -e DURATION=60s browse.js
// Sign-in goes through forward auth, so the pod must be a trusted proxy.
import http from 'k6/http';
import { check } from 'k6';

const BASE = __ENV.BASE || 'http://rustyfin-server';
const IMG = 'EnableImageTypes=Primary,Backdrop,Logo&ImageTypeLimit=3';
const GRID = 'Fields=RecursiveItemCount,ChildCount,OriginalTitle,SortName,Overview,DateCreated';

export const options = {
  scenarios: {
    browse: {
      executor: 'constant-vus',
      vus: Number(__ENV.VUS || 20),
      duration: __ENV.DURATION || '60s',
    },
  },
  summaryTrendStats: ['avg', 'p(50)', 'p(95)', 'p(99)', 'max'],
};

function auth(token) {
  return { headers: { Authorization: `MediaBrowser Client="k6", Device="k6", DeviceId="k6", Version="1", Token="${token}"` } };
}

export function setup() {
  const res = http.post(`${BASE}/Users/AuthenticateByName`, JSON.stringify({ Username: 'stress', Pw: '' }), {
    headers: {
      'Content-Type': 'application/json',
      Authorization: 'MediaBrowser Client="k6", Device="k6", DeviceId="k6", Version="1"',
      'Tailscale-User-Login': 'stress@example.com',
      'Tailscale-User-Name': 'stress',
      'Tailscale-User-Groups': 'admin',
    },
  });
  check(res, { 'signed in': (r) => r.status === 200 });
  const token = res.json('AccessToken');
  const user = res.json('User.Id');
  const p = auth(token);
  const views = http.get(`${BASE}/Users/${user}/Views`, p).json('Items');
  const lib = (type) => (views.find((v) => v.CollectionType === type) || {}).Id;
  const ids = (q) => http.get(`${BASE}/Items?Recursive=true&Limit=200&${q}`, p).json('Items').map((i) => i.Id);
  return {
    token,
    user,
    movies: lib('movies'),
    tv: lib('tvshows'),
    music: lib('music'),
    films: ids('IncludeItemTypes=Movie&SortBy=Random'),
    shows: ids('IncludeItemTypes=Series&SortBy=Random'),
    albums: ids('IncludeItemTypes=MusicAlbum&SortBy=Random'),
    filmCount: http.get(`${BASE}/Items?Recursive=true&Limit=0&IncludeItemTypes=Movie`, p).json('TotalRecordCount'),
  };
}

const pick = (a) => a[Math.floor(Math.random() * a.length)];

export default function (d) {
  const p = auth(d.token);
  const u = d.user;
  const get = (name, url) => {
    const r = http.get(`${BASE}${url}`, Object.assign({ tags: { name } }, p));
    check(r, { [`${name} 200`]: (x) => x.status === 200 });
  };
  switch (Math.floor(Math.random() * 10)) {
    case 0:
      get('home resume', `/UserItems/Resume?UserId=${u}&Limit=21&Fields=Overview,DateCreated&MediaTypes=Video&Recursive=true&EnableTotalRecordCount=false&${IMG}`);
      get('home nextup', `/Shows/NextUp?UserId=${u}&Limit=21&Fields=Overview,DateCreated&EnableResumable=false&NextUpDateCutoff=2025-11-16T00:00:00Z&EnableTotalRecordCount=false&${IMG}`);
      break;
    case 1:
      get('home latest', `/Users/${u}/Items/Latest?Limit=20&ParentId=${pick([d.movies, d.tv, d.music])}&Fields=Overview,DateCreated&${IMG}`);
      break;
    case 2:
    case 3: {
      const start = Math.floor(Math.random() * Math.max(1, d.filmCount - 72));
      get('grid movies', `/Items?UserId=${u}&ParentId=${d.movies}&Recursive=true&StartIndex=${start}&Limit=72&EnableTotalRecordCount=true&IncludeItemTypes=Movie&${GRID}&${IMG}`);
      break;
    }
    case 4:
      get('grid artists', `/Artists/AlbumArtists?UserId=${u}&ParentId=${d.music}&StartIndex=${Math.floor(Math.random() * 1400)}&Limit=72&EnableTotalRecordCount=true&${GRID}&${IMG}`);
      break;
    case 5:
      get('detail film', `/Users/${u}/Items/${pick(d.films)}?Fields=Overview,Genres,People,Studios,DateCreated,MediaSources,ProviderIds`);
      break;
    case 6: {
      const show = pick(d.shows);
      get('detail show', `/Users/${u}/Items/${show}`);
      get('show seasons', `/Shows/${show}/Seasons?UserId=${u}&${GRID}&${IMG}`);
      get('show episodes', `/Shows/${show}/Episodes?UserId=${u}&Fields=UserData,PremiereDate&Limit=200&${IMG}`);
      break;
    }
    case 7:
      get('album tracks', `/Items?UserId=${u}&AlbumIds=${pick(d.albums)}&IncludeItemTypes=Audio&Recursive=true&SortBy=ParentIndexNumber,IndexNumber,SortName&Fields=UserData&${IMG}`);
      break;
    case 8:
      get('search items', `/Items?UserId=${u}&SearchTerm=${encodeURIComponent('Film ' + Math.floor(Math.random() * 2700))}&Recursive=true&Limit=100&EnableTotalRecordCount=false&${IMG}`);
      break;
    default:
      get('search hints', `/Search/Hints?searchTerm=${encodeURIComponent('Track ' + Math.floor(Math.random() * 10))}&Limit=24`);
  }
}
