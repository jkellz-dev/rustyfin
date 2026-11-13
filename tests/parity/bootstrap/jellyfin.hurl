# Brings a fresh Jellyfin to the state the parity suite compares against:
# wizard finished, one admin user, three libraries over the synthetic media.
# Variables: base_url, user, password.

GET {{base_url}}/System/Info/Public
HTTP 200
[Asserts]
jsonpath "$.StartupWizardCompleted" == false

POST {{base_url}}/Startup/Configuration
Content-Type: application/json
{ "UICulture": "en-US", "MetadataCountryCode": "US", "PreferredMetadataLanguage": "en" }
HTTP 204

GET {{base_url}}/Startup/User
HTTP 200

POST {{base_url}}/Startup/User
Content-Type: application/json
{ "Name": "{{user}}", "Password": "{{password}}" }
HTTP 204

POST {{base_url}}/Startup/Complete
HTTP 204

POST {{base_url}}/Users/AuthenticateByName
Content-Type: application/json
Authorization: MediaBrowser Client="parity", Device="parity", DeviceId="parity-1", Version="1.0"
{ "Username": "{{user}}", "Pw": "{{password}}" }
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
paths: /media/parity-media/movies
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/VirtualFolders
Authorization: MediaBrowser Token="{{token}}"
[QueryStringParams]
name: Shows
collectionType: tvshows
refreshLibrary: false
paths: /media/parity-media/shows
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/VirtualFolders
Authorization: MediaBrowser Token="{{token}}"
[QueryStringParams]
name: Music
collectionType: music
refreshLibrary: false
paths: /media/parity-media/music
Content-Type: application/json
{ "LibraryOptions": {} }
HTTP 204

POST {{base_url}}/Library/Refresh
Authorization: MediaBrowser Token="{{token}}"
HTTP 204
