use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::browse::{self, ChildQuery};
use crate::types::*;

// ── View state machine ──────────────────────────────────

#[derive(Clone, PartialEq)]
enum MediaView {
    Libraries,
    Grid,
    Detail(String),
    Search,
}

#[derive(Clone, PartialEq)]
struct Breadcrumb {
    label: String,
    parent_id: Option<String>,
    /// What this level lists when it is opened again.
    query: ChildQuery,
    /// The item open in the detail view (always the last crumb). It lets the
    /// level above it, e.g. the episode's season, be clicked.
    is_detail: bool,
}

// ── Helpers ─────────────────────────────────────────────

fn format_runtime(ticks: i64) -> String {
    let total_minutes = ticks / 10_000_000 / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn format_chapter_time(ticks: i64) -> String {
    let total_seconds = ticks / 10_000_000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The item's primary image at `width`, tagged so the browser keeps it.
fn primary_image_url(item_id: &str, tag: Option<&str>, width: u32) -> String {
    let token = api::get_token().unwrap_or_default();
    browse::image_url(&api::base_url(), item_id, tag, width, &token)
}

fn primary_tag(item: &BaseItemDto) -> Option<String> {
    item.image_tags
        .as_ref()
        .and_then(|tags| tags.get("Primary"))
        .cloned()
}

fn has_primary_image(item: &BaseItemDto) -> bool {
    item.image_tags
        .as_ref()
        .is_some_and(|tags| tags.contains_key("Primary"))
}

fn icon_for_collection(ctype: Option<&str>) -> &'static str {
    match ctype {
        Some("movies") => "🎬",
        Some("tvshows") => "📺",
        Some("music") => "🎵",
        Some("books") => "📚",
        _ => "📁",
    }
}

fn icon_for_item_type(item_type: &str) -> &'static str {
    match item_type {
        "Movie" => "🎬",
        "Series" => "📺",
        "Season" => "📂",
        "Episode" => "🎞",
        "Audio" => "🎵",
        "MusicAlbum" => "💿",
        "MusicArtist" => "🎤",
        _ => "📄",
    }
}

const PAGE_SIZE: i32 = 50;

// ── Component ───────────────────────────────────────────

#[component]
pub fn Media() -> View {
    let current_view: Signal<MediaView> = create_signal(MediaView::Libraries);
    let libraries: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let items: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let total_count = create_signal(0i32);
    let start_index = create_signal(0i32);
    let detail_item: Signal<Option<BaseItemDto>> = create_signal(None);
    let detail_tracks: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let search_results: Signal<Vec<SearchHint>> = create_signal(Vec::new());
    let breadcrumbs: Signal<Vec<Breadcrumb>> = create_signal(Vec::new());
    let current_parent_id: Signal<Option<String>> = create_signal(None);
    let current_query: Signal<ChildQuery> = create_signal(browse::library_query(None));
    let loading = create_signal(true);
    let detail_loading = create_signal(false);
    let search_loading = create_signal(false);
    let refreshing = create_signal(false);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);
    let search_term = create_signal(String::new());

    // Authenticate then load library views
    spawn_local({
        async move {
            if let Err(e) = api::ensure_authenticated().await {
                error.set(Some(e));
                loading.set(false);
                return;
            }

            match api::get_user_views().await {
                Ok(result) => libraries.set(result.items),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        }
    });

    // ── Actions ─────────────────────────────────────────

    let load_items = move |parent_id: String, query: ChildQuery, start: i32| {
        loading.set(true);
        spawn_local({
            let parent_id = parent_id.clone();
            async move {
                match api::get_items(
                    &parent_id,
                    query.item_types,
                    start,
                    PAGE_SIZE,
                    query.sort_by,
                    "Ascending",
                    query.recursive,
                )
                .await
                {
                    Ok(result) => {
                        total_count.set(result.total_record_count);
                        start_index.set(result.start_index);
                        items.set(result.items);
                    }
                    Err(e) => error.set(Some(e)),
                }
                loading.set(false);
            }
        });
    };

    let select_library = move |lib: BaseItemDto| {
        let lib_id = lib.id.clone();
        let lib_name = lib.name.clone();
        let query = browse::library_query(lib.collection_type.as_deref());
        current_parent_id.set(Some(lib_id.clone()));
        current_query.set(query);
        breadcrumbs.set(vec![Breadcrumb {
            label: lib_name,
            parent_id: Some(lib_id.clone()),
            query,
            is_detail: false,
        }]);
        start_index.set(0);
        load_items(lib_id, query, 0);
        current_view.set(MediaView::Grid);
    };

    let browse_into = move |item: BaseItemDto| {
        let item_id = item.id.clone();
        let item_name = item.name.clone();
        // Only the folder's direct children: a show's seasons, a season's episodes.
        let query = browse::children_query(&item.item_type);
        current_parent_id.set(Some(item_id.clone()));
        current_query.set(query);
        breadcrumbs.update(|bc| {
            bc.retain(|c| !c.is_detail);
            bc.push(Breadcrumb {
                label: item_name,
                parent_id: Some(item_id.clone()),
                query,
                is_detail: false,
            });
        });
        start_index.set(0);
        load_items(item_id, query, 0);
    };

    let view_detail = move |item_id: String| {
        detail_loading.set(true);
        detail_tracks.set(Vec::new());
        current_view.set(MediaView::Detail(item_id.clone()));
        spawn_local({
            let item_id = item_id.clone();
            async move {
                match api::get_item(&item_id).await {
                    Ok(item) => {
                        // For albums, fetch child tracks
                        if item.item_type == "MusicAlbum"
                            && let Ok(result) = api::get_items(
                                &item.id,
                                None,
                                0,
                                500,
                                "IndexNumber",
                                "Ascending",
                                false,
                            )
                            .await
                        {
                            detail_tracks.set(result.items);
                        }
                        let label =
                            browse::card_label(&item.item_type, &item.name, item.index_number);
                        let query = current_query.get();
                        breadcrumbs.update(|bc| {
                            bc.retain(|c| !c.is_detail);
                            bc.push(Breadcrumb {
                                label,
                                parent_id: None,
                                query,
                                is_detail: true,
                            });
                        });
                        detail_item.set(Some(item));
                    }
                    Err(e) => error.set(Some(e)),
                }
                detail_loading.set(false);
            }
        });
    };

    let navigate_breadcrumb = move |index: usize| {
        if index == 0 && breadcrumbs.get_clone().len() > 1 {
            // Back to first breadcrumb level (library root)
            let crumbs = breadcrumbs.get_clone();
            let crumb = &crumbs[0];
            let parent_id = crumb.parent_id.clone().unwrap_or_default();
            let query = crumb.query;
            current_parent_id.set(Some(parent_id.clone()));
            current_query.set(query);
            breadcrumbs.update(|bc| bc.truncate(1));
            start_index.set(0);
            load_items(parent_id, query, 0);
            current_view.set(MediaView::Grid);
        } else if index < breadcrumbs.get_clone().len() - 1 {
            let crumbs = breadcrumbs.get_clone();
            let crumb = &crumbs[index];
            let parent_id = crumb.parent_id.clone().unwrap_or_default();
            let query = crumb.query;
            current_parent_id.set(Some(parent_id.clone()));
            current_query.set(query);
            breadcrumbs.update(|bc| bc.truncate(index + 1));
            start_index.set(0);
            load_items(parent_id, query, 0);
            current_view.set(MediaView::Grid);
        }
    };

    let back_to_libraries = move |_| {
        breadcrumbs.set(Vec::new());
        current_parent_id.set(None);
        current_query.set(browse::library_query(None));
        search_term.set(String::new());
        search_results.set(Vec::new());
        current_view.set(MediaView::Libraries);
    };

    let do_search = move || {
        let term = search_term.get_clone();
        if term.trim().is_empty() {
            return;
        }
        search_loading.set(true);
        current_view.set(MediaView::Search);
        spawn_local({
            async move {
                match api::search_hints(&term).await {
                    Ok(result) => search_results.set(result.search_hints),
                    Err(e) => error.set(Some(e)),
                }
                search_loading.set(false);
            }
        });
    };

    let on_search_keydown = move |e: web_sys::KeyboardEvent| {
        if e.key() == "Enter" {
            do_search();
        }
    };

    let on_refresh = move |item_id: String| {
        refreshing.set(true);
        spawn_local({
            async move {
                match api::refresh_item(&item_id).await {
                    Ok(()) => toast.set(Some("Metadata refresh started".to_string())),
                    Err(e) => error.set(Some(e)),
                }
                refreshing.set(false);
            }
        });
    };

    // ── Render ──────────────────────────────────────────

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                spawn_local({
                    async move {
                        gloo_net::http::Request::get("data:,")
                            .send().await.ok();
                        toast.set(None);
                    }
                });
                view! {
                    div(class="toast-container") {
                        div(class="toast toast-success") { (msg) }
                    }
                }
            } else {
                view! {}
            })

            // Error
            (if let Some(err) = error.get_clone() {
                view! {
                    div(class="card mb-16") {
                        p(class="text-muted") { (err) }
                    }
                }
            } else {
                view! {}
            })

            // Page header with search
            div(class="page-header") {
                h1 { "Media" }
                div(class="media-search") {
                    input(
                        class="search-input",
                        r#type="text",
                        placeholder="Search media...",
                        bind:value=search_term,
                        on:keydown=on_search_keydown,
                    )
                }
            }

            // Breadcrumbs (show when not on libraries view)
            (if current_view.get_clone() != MediaView::Libraries {
                view! {
                    div(class="breadcrumbs") {
                        button(class="breadcrumb-link", on:click=back_to_libraries) { "Libraries" }
                        Indexed(
                            list=move || browse::crumb_trail(&breadcrumbs.get_clone()),
                            view=move |entry: (usize, Breadcrumb, bool)| {
                                let (idx, crumb, is_last) = entry;
                                let label = crumb.label.clone();
                                if is_last {
                                    view! {
                                        span(class="breadcrumb-sep") { " / " }
                                        span(class="breadcrumb-current") { (label) }
                                    }
                                } else {
                                    view! {
                                        span(class="breadcrumb-sep") { " / " }
                                        button(class="breadcrumb-link", on:click=move |_| navigate_breadcrumb(idx)) {
                                            (label)
                                        }
                                    }
                                }
                            }
                        )
                    }
                }
            } else {
                view! {}
            })

            // Main content
            (match current_view.get_clone() {
                MediaView::Libraries => {
                    if loading.get() {
                        view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
                    } else {
                        let lib_list = libraries.get_clone();
                        if lib_list.is_empty() {
                            view! { div(class="empty-state") { "No libraries found. Add libraries first." } }
                        } else {
                            view! {
                                div(class="library-grid") {
                                    Indexed(
                                        list=move || lib_list.clone(),
                                        view=move |lib: BaseItemDto| {
                                            let icon = icon_for_collection(lib.collection_type.as_deref());
                                            let name = lib.name.clone();
                                            let ctype_label = lib.collection_type.clone().unwrap_or_else(|| "Mixed".to_string());
                                            let lib_clone = lib.clone();
                                            view! {
                                                div(class="library-card", on:click=move |_| select_library(lib_clone.clone())) {
                                                    span(class="library-card-icon") { (icon) }
                                                    div(class="library-card-name") { (name) }
                                                    div(class="library-card-type") { (ctype_label) }
                                                }
                                            }
                                        }
                                    )
                                }
                            }
                        }
                    }
                },
                MediaView::Grid => {
                    if loading.get() {
                        view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
                    } else {
                        let item_list = items.get_clone();
                        let total = total_count.get();
                        let start = start_index.get();
                        if item_list.is_empty() {
                            view! { div(class="empty-state") { "No items found." } }
                        } else {
                            view! {
                                div {
                                    div(class="media-grid") {
                                        Indexed(
                                            list=move || item_list.clone(),
                                            view=move |item: BaseItemDto| {
                                                let item_id = item.id.clone();
                                                let item_name = item.name.clone();
                                                let year = item.production_year;
                                                let is_folder = item.is_folder.unwrap_or(false);
                                                let has_image = has_primary_image(&item);
                                                let img_url = primary_image_url(&item.id, primary_tag(&item).as_deref(), browse::CARD_IMAGE_WIDTH);
                                                let item_type = item.item_type.clone();
                                                let item_for_click = item.clone();
                                                let child_count = item.child_count;

                                                let display_name =
                                                    browse::card_label(&item_type, &item_name, item.index_number);

                                                let (poster_class, placeholder_class) = browse::grid_classes(&item_type);

                                                let is_album = item_type == "MusicAlbum";
                                                let on_click = move |_| {
                                                    if is_album || !is_folder {
                                                        view_detail(item_id.clone());
                                                    } else {
                                                        browse_into(item_for_click.clone());
                                                    }
                                                };

                                                view! {
                                                    div(class="media-card", on:click=on_click) {
                                                        (if has_image {
                                                            let url = img_url.clone();
                                                            view! { img(class=poster_class, src=url, loading="lazy") }
                                                        } else {
                                                            let icon = icon_for_item_type(&item_type);
                                                            view! { div(class=placeholder_class) { (icon) } }
                                                        })
                                                        div(class="media-card-title") { (display_name) }
                                                        (if let Some(y) = year {
                                                            view! { div(class="media-card-year") { (y) } }
                                                        } else if let Some(count) = child_count {
                                                            view! { div(class="media-card-year") { (count) " items" } }
                                                        } else {
                                                            view! {}
                                                        })
                                                    }
                                                }
                                            }
                                        )
                                    }

                                    // Pagination
                                    (if total > PAGE_SIZE {
                                        let has_prev = start > 0;
                                        let has_next = start + PAGE_SIZE < total;
                                        let showing_end = std::cmp::min(start + PAGE_SIZE, total);
                                        let pid = current_parent_id.get_clone().unwrap_or_default();
                                        let query = current_query.get();
                                        let pid2 = pid.clone();
                                        view! {
                                            div(class="pagination") {
                                                button(
                                                    class="btn btn-secondary btn-sm",
                                                    disabled=!has_prev,
                                                    on:click=move |_| {
                                                        let new_start = (start - PAGE_SIZE).max(0);
                                                        start_index.set(new_start);
                                                        load_items(pid.clone(), query, new_start);
                                                    }
                                                ) { "Previous" }
                                                span(class="pagination-info") {
                                                    (start + 1) " - " (showing_end) " of " (total)
                                                }
                                                button(
                                                    class="btn btn-secondary btn-sm",
                                                    disabled=!has_next,
                                                    on:click=move |_| {
                                                        let new_start = start + PAGE_SIZE;
                                                        start_index.set(new_start);
                                                        load_items(pid2.clone(), query, new_start);
                                                    }
                                                ) { "Next" }
                                            }
                                        }
                                    } else {
                                        view! {}
                                    })
                                }
                            }
                        }
                    }
                },
                MediaView::Detail(ref item_id) => {
                    let detail_id = item_id.clone();
                    if detail_loading.get() {
                        view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
                    } else if let Some(item) = detail_item.get_clone() {
                        let has_image = has_primary_image(&item);
                        let img_url = primary_image_url(&item.id, primary_tag(&item).as_deref(), browse::DETAIL_IMAGE_WIDTH);
                        let refresh_id = detail_id.clone();
                        let detail_item_type = item.item_type.clone();
                        let is_album = detail_item_type == "MusicAlbum";
                        let (detail_poster_class, detail_placeholder_class) =
                            browse::detail_classes(&detail_item_type);

                        view! {
                            div(class="detail-page") {
                                div(class="detail-header") {
                                    div(class=detail_poster_class) {
                                        (if has_image {
                                            let url = img_url.clone();
                                            view! { img(src=url) }
                                        } else {
                                            let icon = icon_for_item_type(&item.item_type);
                                            view! { div(class=detail_placeholder_class) { (icon) } }
                                        })
                                    }
                                    div(class="detail-meta") {
                                        h1 { (item.name.clone()) }

                                        // Subtitle (series name, season/episode info)
                                        (if let Some(ref series) = item.series_name {
                                            let mut sub = series.clone();
                                            if let Some(s) = item.parent_index_number {
                                                sub.push_str(&format!(" — S{s:02}"));
                                            }
                                            if let Some(e) = item.index_number {
                                                sub.push_str(&format!("E{e:02}"));
                                            }
                                            view! { div(class="subtitle") { (sub) } }
                                        } else {
                                            view! {}
                                        })

                                        // Badges
                                        div(class="detail-badges") {
                                            (if let Some(rating) = item.community_rating {
                                                let r = format!("{rating:.1}");
                                                view! { span(class="badge badge-rating") { "★ " (r) } }
                                            } else {
                                                view! {}
                                            })
                                            ({
                                                let cert = item.official_rating.clone();
                                                if let Some(c) = cert {
                                                    view! { span(class="badge badge-type") { (c) } }
                                                } else {
                                                    view! {}
                                                }
                                            })
                                            (if let Some(y) = item.production_year {
                                                view! { span(class="badge badge-year") { (y) } }
                                            } else {
                                                view! {}
                                            })
                                            (if let Some(ticks) = item.runtime_ticks {
                                                let rt = format_runtime(ticks);
                                                view! { span(class="badge badge-runtime") { (rt) } }
                                            } else {
                                                view! {}
                                            })
                                            ({
                                                let container = item.container.clone();
                                                if let Some(c) = container {
                                                    let upper = c.to_uppercase();
                                                    view! { span(class="badge badge-type") { (upper) } }
                                                } else {
                                                    view! {}
                                                }
                                            })
                                        }

                                        // Actions
                                        div(class="detail-actions") {
                                            button(
                                                class="btn btn-primary btn-sm",
                                                disabled=refreshing.get(),
                                                on:click={
                                                    let rid = refresh_id.clone();
                                                    move |_| on_refresh(rid.clone())
                                                }
                                            ) {
                                                (if refreshing.get() { "Refreshing..." } else { "Refresh Metadata" })
                                            }
                                            button(
                                                class="btn btn-secondary btn-sm",
                                                on:click=move |_| {
                                                    // Back to the level the item was opened from
                                                    breadcrumbs.update(|bc| bc.retain(|c| !c.is_detail));
                                                    current_view.set(MediaView::Grid);
                                                }
                                            ) { "Back" }
                                        }

                                        // Genres
                                        (if !item.genres.is_empty() {
                                            let genres = item.genres.join(", ");
                                            view! {
                                                div(class="detail-section") {
                                                    h3 { "Genres" }
                                                    p { (genres) }
                                                }
                                            }
                                        } else {
                                            view! {}
                                        })

                                        // Studios
                                        (if !item.studios.is_empty() {
                                            let studio_names: Vec<String> = item.studios.iter().map(|s| s.name.clone()).collect();
                                            let studios = studio_names.join(", ");
                                            view! {
                                                div(class="detail-section") {
                                                    h3 { "Studios" }
                                                    p { (studios) }
                                                }
                                            }
                                        } else {
                                            view! {}
                                        })
                                    }
                                }

                                // Overview
                                ({
                                    let overview = item.overview.clone();
                                    if let Some(o) = overview {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "Overview" }
                                                p(class="detail-overview") { (o) }
                                            }
                                        }
                                    } else {
                                        view! {}
                                    }
                                })

                                // Tracklist (for albums)
                                (if is_album {
                                    let tracks = detail_tracks.get_clone();
                                    if tracks.is_empty() {
                                        view! {}
                                    } else {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "Tracks" }
                                                div(class="tracklist") {
                                                    Indexed(
                                                        list=move || tracks.clone(),
                                                        view=move |track: BaseItemDto| {
                                                            let num_str = track.index_number.map(|n| n.to_string()).unwrap_or_default();
                                                            let name = track.name.clone();
                                                            let duration_str = track.runtime_ticks.map(format_runtime).unwrap_or_default();
                                                            view! {
                                                                div(class="track-row") {
                                                                    span(class="track-num") { (num_str) }
                                                                    span(class="track-name") { (name) }
                                                                    span(class="track-duration") { (duration_str) }
                                                                }
                                                            }
                                                        }
                                                    )
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    view! {}
                                })

                                // Media Streams
                                (if !item.media_streams.is_empty() {
                                    let streams = item.media_streams.clone();
                                    view! {
                                        div(class="detail-section") {
                                            h3 { "Media Streams" }
                                            table(class="stream-table") {
                                                thead {
                                                    tr {
                                                        th { "#" }
                                                        th { "Type" }
                                                        th { "Codec" }
                                                        th { "Details" }
                                                    }
                                                }
                                                tbody {
                                                    Indexed(
                                                        list=move || streams.clone(),
                                                        view=move |stream: MediaStreamDto| {
                                                            let idx = stream.index;
                                                            let stype = stream.stream_type.clone();
                                                            let codec = stream.codec.clone().unwrap_or_default();
                                                            let details = stream_details(&stream);
                                                            view! {
                                                                tr {
                                                                    td { (idx) }
                                                                    td { (stype) }
                                                                    td { (codec) }
                                                                    td { (details) }
                                                                }
                                                            }
                                                        }
                                                    )
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    view! {}
                                })

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
                                                        let has_image = person.id.is_some() && person.primary_image_tag.is_some();
                                                        let img_url = if has_image {
                                                            let id = person.id.as_deref().unwrap_or_default();
                                                            let tag = person.primary_image_tag.as_deref().unwrap_or_default();
                                                            format!("/Items/{id}/Images/Primary?tag={tag}&maxWidth=160&quality=90")
                                                        } else {
                                                            String::new()
                                                        };
                                                        let initials = {
                                                            let parts: Vec<&str> = person.name.split_whitespace().collect();
                                                            let first = parts.first().and_then(|w| w.chars().next()).unwrap_or(' ');
                                                            let last = if parts.len() > 1 {
                                                                parts.last().and_then(|w| w.chars().next()).unwrap_or(' ')
                                                            } else {
                                                                ' '
                                                            };
                                                            format!("{first}{last}").trim().to_uppercase()
                                                        };
                                                        let bg_color = {
                                                            let hash = person.name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
                                                            let hue = hash % 360;
                                                            format!("hsl({hue}, 40%, 30%)")
                                                        };
                                                        let subtitle = if person.person_type.as_deref() == Some("Actor") {
                                                            person.role.clone().unwrap_or_default()
                                                        } else {
                                                            person.person_type.clone().unwrap_or_default()
                                                        };
                                                        let alt_text = name.clone();
                                                        let title_text = name.clone();
                                                        let label_text = name;
                                                        view! {
                                                            div(class="person-card") {
                                                                (if has_image {
                                                                    let src = img_url.clone();
                                                                    let alt = alt_text.clone();
                                                                    view! {
                                                                        img(class="person-avatar", src=src, alt=alt, loading="lazy")
                                                                    }
                                                                } else {
                                                                    let style = format!("background-color: {};", bg_color);
                                                                    let ini = initials.clone();
                                                                    view! {
                                                                        div(class="person-avatar person-initials", style=style) {
                                                                            (ini)
                                                                        }
                                                                    }
                                                                })
                                                                div(class="person-info") {
                                                                    span(class="person-name", title=title_text) { (label_text) }
                                                                    (if !subtitle.is_empty() {
                                                                        let sub = subtitle.clone();
                                                                        view! { span(class="person-role") { (sub) } }
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

                                // Chapters
                                (if !item.chapters.is_empty() {
                                    let chapters = item.chapters.clone();
                                    view! {
                                        div(class="detail-section") {
                                            h3 { "Chapters" }
                                            div(class="chapter-list") {
                                                Indexed(
                                                    list=move || chapters.clone(),
                                                    view=move |ch: ChapterInfoDto| {
                                                        let time = format_chapter_time(ch.start_position_ticks);
                                                        let name = ch.name.clone().unwrap_or_else(|| "Chapter".to_string());
                                                        view! {
                                                            div(class="chapter-item") {
                                                                span(class="chapter-time") { (time) }
                                                                span { (name) }
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

                                // Provider IDs
                                (if let Some(ref providers) = item.provider_ids {
                                    if !providers.is_empty() {
                                        let provs: Vec<(String, String)> = providers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "External IDs" }
                                                div(class="provider-list") {
                                                    Indexed(
                                                        list=move || provs.clone(),
                                                        view=move |prov: (String, String)| {
                                                            let (name, id) = prov;
                                                            view! {
                                                                span(class="provider-chip") {
                                                                    span(class="provider-name") { (name.clone()) ":" }
                                                                    " " (id)
                                                                }
                                                            }
                                                        }
                                                    )
                                                }
                                            }
                                        }
                                    } else {
                                        view! {}
                                    }
                                } else {
                                    view! {}
                                })

                                // File path
                                ({
                                    let file_path = item.path.clone();
                                    if let Some(p) = file_path {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "File" }
                                                p(class="text-muted") { (p) }
                                            }
                                        }
                                    } else {
                                        view! {}
                                    }
                                })
                            }
                        }
                    } else {
                        view! { div(class="empty-state") { "Item not found." } }
                    }
                },
                MediaView::Search => {
                    if search_loading.get() {
                        view! { div(class="loading-container") { span(class="spinner") {} " Searching..." } }
                    } else {
                        let results = search_results.get_clone();
                        if results.is_empty() {
                            view! { div(class="empty-state") { "No results found." } }
                        } else {
                            view! {
                                div(class="search-results") {
                                    Indexed(
                                        list=move || results.clone(),
                                        view=move |hint: SearchHint| {
                                            let hint_id = hint.id.clone();
                                            let name = hint.name.clone();
                                            let item_type = hint.item_type.clone();
                                            let year = hint.production_year;
                                            let series = hint.series_name.clone();
                                            let has_image = hint.primary_image_tag.is_some();
                                            let img_url = primary_image_url(&hint.id, hint.primary_image_tag.as_deref(), browse::THUMB_IMAGE_WIDTH);

                                            let mut meta_parts = Vec::new();
                                            meta_parts.push(item_type.clone());
                                            if let Some(ref s) = series {
                                                meta_parts.push(s.clone());
                                            }
                                            if let Some(y) = year {
                                                meta_parts.push(y.to_string());
                                            }
                                            if let Some(ticks) = hint.run_time_ticks {
                                                meta_parts.push(format_runtime(ticks));
                                            }
                                            let meta = meta_parts.join(" · ");

                                            view! {
                                                div(
                                                    class="search-result-item",
                                                    on:click=move |_| view_detail(hint_id.clone())
                                                ) {
                                                    (if has_image {
                                                        let url = img_url.clone();
                                                        view! { img(class="search-result-poster", src=url, loading="lazy") }
                                                    } else {
                                                        let icon = icon_for_item_type(&item_type);
                                                        view! { div(class="search-result-poster media-poster-placeholder") { (icon) } }
                                                    })
                                                    div(class="search-result-info") {
                                                        div(class="name") { (name) }
                                                        div(class="meta") { (meta) }
                                                    }
                                                }
                                            }
                                        }
                                    )
                                }
                            }
                        }
                    }
                },
            })
        }
    }
}

// ── Stream detail formatter ─────────────────────────────

fn stream_details(s: &MediaStreamDto) -> String {
    let mut parts = Vec::new();
    match s.stream_type.as_str() {
        "Video" => {
            if let (Some(w), Some(h)) = (s.width, s.height) {
                parts.push(format!("{w}x{h}"));
            }
            if let Some(fps) = s.real_frame_rate {
                parts.push(format!("{fps:.1} fps"));
            }
            if let Some(br) = s.bit_rate {
                parts.push(format!("{:.1} Mbps", br as f64 / 1_000_000.0));
            }
        }
        "Audio" => {
            if let Some(ref lang) = s.language {
                parts.push(lang.clone());
            }
            if let Some(ch) = s.channels {
                parts.push(format!("{ch}ch"));
            }
            if let Some(ref layout) = s.channel_layout {
                parts.push(layout.clone());
            }
            if let Some(sr) = s.sample_rate {
                parts.push(format!("{sr} Hz"));
            }
        }
        "Subtitle" => {
            if let Some(ref lang) = s.language {
                parts.push(lang.clone());
            }
            if let Some(ref title) = s.title {
                parts.push(title.clone());
            }
        }
        _ => {
            if let Some(ref lang) = s.language {
                parts.push(lang.clone());
            }
        }
    }
    if s.is_default == Some(true) {
        parts.push("default".to_string());
    }
    parts.join(" · ")
}
