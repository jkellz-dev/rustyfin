# Cast & Crew Headshots — Frontend Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace text-only person chips in the admin UI movie detail page with portrait cards showing headshot images, name, and role.

**Architecture:** Frontend-only change. The backend already serves `BaseItemPerson` with `id` and `primary_image_tag`, and the person image endpoint (`/Items/{person_id}/Images/Primary`) is functional. We replace the `.person-chip` spans with portrait cards that construct image URLs from the DTO fields, falling back to initials avatars when no image exists.

**Tech Stack:** Sycamore (Rust WASM), CSS

---

### Task 1: Replace person chips with portrait cards in media.rs

**Files:**
- Modify: `frontend/src/pages/media.rs:745-774`

**Context:** The current cast section (lines 745-774) renders an `Indexed` list of `BaseItemPerson` as `span(class="person-chip")` elements containing just name and role text. We need to replace this with portrait cards that show an image (or initials fallback), name, and role/type.

The image URL pattern for a person with a headshot is:
```
/Items/{person_id}/Images/Primary?tag={tag}
```
where `person_id` comes from `BaseItemPerson.id` (a `Option<String>` UUID) and `tag` comes from `BaseItemPerson.primary_image_tag`.

For actors, show the role. For directors/writers/other crew, show the person type instead.

**Step 1: Replace the cast rendering block**

Replace lines 745-774 in `frontend/src/pages/media.rs` with:

```rust
// Cast / People
(if !item.people.is_empty() {
    let people = item.people.clone();
    view! {
        div(class="detail-section") {
            h3 { "Cast & Crew" }
            div(class="people-grid") {
                Indexed(
                    list=move || people.clone(),
                    view=move |person: BaseItemPerson| {
                        let name = person.name.clone();
                        let display_name = person.name.clone();
                        let role_or_type = if person.person_type.as_deref() == Some("Actor") {
                            person.role.clone()
                        } else {
                            person.person_type.clone()
                        };

                        let has_image = person.id.is_some()
                            && person.primary_image_tag.is_some();
                        let image_url = if let (Some(ref id), Some(ref tag)) =
                            (&person.id, &person.primary_image_tag)
                        {
                            format!("/Items/{id}/Images/Primary?tag={tag}&maxWidth=160&quality=90")
                        } else {
                            String::new()
                        };

                        // Initials: first char of first + last word
                        let initials = {
                            let parts: Vec<&str> = name.split_whitespace().collect();
                            let first = parts.first()
                                .and_then(|s| s.chars().next())
                                .unwrap_or('?');
                            let last = if parts.len() > 1 {
                                parts.last()
                                    .and_then(|s| s.chars().next())
                                    .unwrap_or(' ')
                            } else {
                                ' '
                            };
                            format!("{first}{last}").trim().to_string()
                        };

                        // Deterministic color from name hash
                        let color_index = name.bytes().fold(0u32, |acc, b| {
                            acc.wrapping_add(b as u32)
                        }) % 8;
                        let bg_colors = [
                            "#5b4a9e", "#2e7d5b", "#8b5e3c",
                            "#5c3d6e", "#2a6496", "#7a5230",
                            "#3b6e5e", "#6b3a5c",
                        ];
                        let bg_color = bg_colors[color_index as usize];

                        view! {
                            div(class="person-card") {
                                (if has_image {
                                    let url = image_url.clone();
                                    view! {
                                        img(
                                            class="person-avatar",
                                            src=url,
                                            alt=display_name.clone(),
                                            loading="lazy",
                                        )
                                    }
                                } else {
                                    let initials_clone = initials.clone();
                                    view! {
                                        div(
                                            class="person-avatar person-initials",
                                            style=format!("background-color: {bg_color};"),
                                        ) {
                                            (initials_clone)
                                        }
                                    }
                                })
                                div(class="person-info") {
                                    span(class="person-name") { (display_name) }
                                    (if let Some(r) = role_or_type {
                                        view! { span(class="person-role") { (r) } }
                                    } else {
                                        view! {}
                                    })
                                }
                            }
                        }
                    }
                )
            }
        }
    }
} else {
    view! {}
})
```

**Step 2: Verify it compiles**

Run: `cd frontend && trunk build 2>&1 | tail -5`
Expected: Build succeeds (or at minimum, no Rust compilation errors — CSS may look wrong until Task 2).

**Step 3: Run cargo fmt and clippy**

Run: `cargo fmt && cargo clippy -- -D warnings`
Expected: Clean.

**Step 4: Commit**

```bash
jj describe -m "feat: replace person chips with portrait cards showing headshots"
jj new
```

---

### Task 2: Replace person-chip CSS with portrait card styles

**Files:**
- Modify: `frontend/style.css:667-685`

**Context:** Replace the existing `.people-grid`, `.person-chip`, and `.person-chip .role` styles with portrait card layout styles. Use the project's existing CSS variables (`--bg-card`, `--border`, `--text-muted`).

**Step 1: Replace the People Grid CSS block**

Replace lines 667-685 in `frontend/style.css` (the entire `/* ── People Grid ── */` section including `.people-grid`, `.person-chip`, and `.person-chip .role`) with:

```css
/* ── People Grid ───────────────────────────────────────── */
.people-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
}

.person-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 80px;
    text-align: center;
}

.person-avatar {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    object-fit: cover;
    background: var(--bg-card);
    border: 2px solid var(--border);
    margin-bottom: 6px;
}

.person-initials {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.4em;
    font-weight: 600;
    color: #fff;
    user-select: none;
}

.person-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 80px;
}

.person-name {
    font-size: 0.78em;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.person-role {
    font-size: 0.7em;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
```

**Step 2: Build and verify visually**

Run: `cd frontend && trunk build`
Expected: Build succeeds. Cards display as circular images with name/role below.

**Step 3: Commit**

```bash
jj describe -m "style: portrait card layout for cast & crew section"
jj new
```

---

### Task 3: Final verification

**Step 1: Run cargo fmt and clippy on the whole workspace**

Run: `cargo fmt && cargo clippy -- -D warnings`
Expected: Clean, no warnings.

**Step 2: Run unit tests**

Run: `cargo test --lib`
Expected: All pass (frontend changes are WASM, not covered by `--lib`).

**Step 3: Build the frontend in release mode**

Run: `cd frontend && trunk build --release`
Expected: Build succeeds.

**Step 4: Manual visual check**

Start the server, navigate to a movie detail page. Verify:
- People with headshots show circular portrait images
- People without headshots show colored initials circles
- Names and roles/types appear below each card
- Long names are truncated with ellipsis
- Layout wraps on narrow screens

**Step 5: Commit any final adjustments**

```bash
jj describe -m "chore: final verification of cast/crew headshots UI"
jj new
```
