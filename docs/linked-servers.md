# Linked Servers

A friend's Jellyfin (or Rustyfin) can share some of its libraries with your Rustyfin. Their films then appear in your
libraries as if they were yours:

- A film you already have gains their copy as another version. The best copy (resolution, then HDR, then bitrate for
  the codec, then audio) is listed first and plays unless you pick another.
- A film you do not have is listed like your own, with its artwork, details and provider ids.
- Your watched state, resume points and favourites stay on your server, whichever copy you played.

Your apps only ever talk to your server: it signs in to the friend's server, keeps a copy of the shared catalogue
(never the media), and passes the friend's posters and video bytes through.

**What works so far:** films, played as is (direct play, with seeking). Converting a friend's copy (transcoding) and
TV shows are next on the [roadmap](../roadmap.md) (16b). A copy your app cannot play as is is listed after the copies
it can play.

## On the friend's server

The friend makes a user for the link and lets it see only the libraries they want to share. In Jellyfin: Dashboard,
Users, add a user, then under Access untick "Enable access to all libraries" and tick the shared ones. An
administrator's API key is not needed and should not be used: it would give your server everything.

## On your server

Add a `[[link]]` for each friend to `rustyfin.toml`:

```toml
[[link]]
name = "Alex"                       # shown beside their copies: "2160p · Alex"
kind = "jellyfin"                   # or "rustyfin"
url = "https://jellyfin.alex.example"
user = "link-for-sam"
password_file = "/run/secrets/alex-link-password"   # or: password = "..."

[[link.libraries]]
remote = "Films"                    # the friend's library, by name
local = "Movies"                    # one of yours, by name
```

Restart Rustyfin. It signs in, syncs 30 seconds after start and then every 15 minutes. Each sync lists the friend's
films with the tag Jellyfin changes when an item does, fetches details only of new and changed films, and removes
the ones no longer shared. A film you add later takes in the friend's copy at the next sync.

If a sync fails (wrong password, the friend's server is down, a library name that does not match), the server log
says why, and the link keeps the films from the last good sync.

## How copies are matched

Only by provider id: the same TMDB or IMDb id. Titles and years are never used, because a wrong match would hide a
film. A friend's film whose ids match two of your films stays on its own. Two friends' copies of a film you lack are
one film too.
