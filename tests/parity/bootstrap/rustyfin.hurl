# Brings a fresh Rustyfin (behind forward auth) to the same state as jellyfin.hurl.
# Rustyfin has no setup wizard and no passwords: the proxy vouches for the caller.
# Variables: base_url, user, media_root (/data/media/parity-media in the sandbox and CI).

GET {{base_url}}/System/Info/Public
HTTP 200

POST {{base_url}}/Users/AuthenticateByName
Content-Type: application/json
Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="parity-1", Version="1.0"
Tailscale-User-Login: {{user}}@example.com
Tailscale-User-Name: {{user}}
Tailscale-User-Groups: admin
{ "Username": "{{user}}", "Pw": "" }
HTTP 200
[Captures]
token: jsonpath "$.AccessToken"
user_id: jsonpath "$.User.Id"

POST {{base_url}}/Library/VirtualFolders
Authorization: MediaBrowser Token="{{token}}"
[QueryStringParams]
name: Movies
collectionType: movies
refreshLibrary: false
paths: {{media_root}}/movies
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/VirtualFolders
Authorization: MediaBrowser Token="{{token}}"
[QueryStringParams]
name: Shows
collectionType: tvshows
refreshLibrary: false
paths: {{media_root}}/shows
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/VirtualFolders
Authorization: MediaBrowser Token="{{token}}"
[QueryStringParams]
name: Music
collectionType: music
refreshLibrary: false
paths: {{media_root}}/music
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/Refresh
Authorization: MediaBrowser Token="{{token}}"
HTTP 204
