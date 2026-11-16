# Waits until a freshly bootstrapped Rustyfin has scanned the synthetic media:
# one movie, two episodes, two tracks. Variables: base_url, user.

POST {{base_url}}/Users/AuthenticateByName
Content-Type: application/json
Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="parity-wait", Version="1.0"
Tailscale-User-Login: {{user}}@example.com
Tailscale-User-Name: {{user}}
Tailscale-User-Groups: admin
{ "Username": "{{user}}", "Pw": "" }
HTTP 200
[Captures]
token: jsonpath "$.AccessToken"

GET {{base_url}}/Items?Recursive=true&IncludeItemTypes=Audio
Authorization: MediaBrowser Token="{{token}}"
[Options]
retry: 60
retry-interval: 2000
HTTP 200
[Asserts]
jsonpath "$.TotalRecordCount" == 2

GET {{base_url}}/Items?Recursive=true&IncludeItemTypes=Episode
Authorization: MediaBrowser Token="{{token}}"
[Options]
retry: 30
retry-interval: 2000
HTTP 200
[Asserts]
jsonpath "$.TotalRecordCount" == 2

GET {{base_url}}/Items?Recursive=true&IncludeItemTypes=Movie
Authorization: MediaBrowser Token="{{token}}"
[Options]
retry: 30
retry-interval: 2000
HTTP 200
[Asserts]
jsonpath "$.TotalRecordCount" == 1
