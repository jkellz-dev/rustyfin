# Cast & Crew Headshots — Design

## Background

The backend pipeline for person headshots is fully implemented:

1. `rustyfin-meta` CLI downloads headshots to `~/.cache/rustyfin/people/{provider_person_id}.jpg`
2. Server enrichment inserts `entity_images` rows for people with cached headshots
3. API populates `BaseItemPerson.primary_image_tag` when an image row exists
4. `/Items/{person_id}/Images/Primary?tag={tag}` serves the file

The admin UI currently renders cast & crew as text-only pill chips with no images.

## Goal

Replace the text-only person chips on the movie detail page with portrait cards showing headshot images, name, and role.

## UI Design

- Horizontal wrapping grid (`flex-wrap`) of portrait cards
- Each card: ~80px wide, circular image (80x80) on top, name below, role/type (muted) below that
- Long names truncated with ellipsis
- Actor cards show role; director/writer cards show person type instead
- Display only — no click behavior

## Image URL Construction

- When `primary_image_tag` is `Some`: `/Items/{person_id}/Images/Primary?tag={tag}`
- When `primary_image_tag` is `None`: initials avatar fallback

## Fallback (No Image)

Initials avatar: colored circle with person's initials (first letter of first + last name). Background color is deterministic from a name hash so the same person always gets the same color.

## Files to Change

- `frontend/src/pages/media.rs` — replace chip rendering with portrait card rendering, add image URL construction and initials fallback logic
- `frontend/style.css` — replace `.person-chip` / `.people-grid` styles with portrait card layout

## Out of Scope

- Person detail page / click navigation
- Server-side changes (pipeline already complete)
- Person image management UI
