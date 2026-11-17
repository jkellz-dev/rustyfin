use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use sycamore_router::navigate;

use crate::api;
use crate::browse::{self, ChildQuery};
use crate::components::error_notice::ErrorNotice;
use crate::components::loading::Loading;
use crate::media_route::{self, Crumb, MediaRoute};
use crate::types::*;

// ── View state machine ──────────────────────────────────

#[derive(Clone, PartialEq)]
enum MediaView {
    Libraries,
    Grid,
    Detail(String),
    Search,
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

/// The chosen value of a `<select>` from its change event.
fn select_value(e: web_sys::Event) -> String {
    e.target()
        .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok())
        .map(|s| s.value())
        .unwrap_or_default()
}

// ── Component ───────────────────────────────────────────

/// The media browser at `path` (an address under `/web/media`, see
/// `media_route`): the router builds a fresh page for each address, so the
/// back and forward buttons move through the browser's places.
#[component(inline_props)]
pub fn Media(path: String) -> View {
    api::new_page();
    let route = MediaRoute::parse(&path);
    let current_view: Signal<MediaView> = create_signal(match &route {
        MediaRoute::Libraries => MediaView::Libraries,
        MediaRoute::Item { id } => MediaView::Detail(id.clone()),
        MediaRoute::Search { .. } => MediaView::Search,
        _ => MediaView::Grid,
    });
    let libraries: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let items: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let total_count = create_signal(0i32);
    let start_index = create_signal(0i32);
    let detail_item: Signal<Option<BaseItemDto>> = create_signal(None);
    let detail_tracks: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    // The collections a film belongs to (links on its page).
    let detail_collections: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let detail_extras: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    let search_results: Signal<Vec<SearchHint>> = create_signal(Vec::new());
    let breadcrumbs: Signal<Vec<Crumb>> = create_signal(Vec::new());
    // The library the page is in, which the search box searches.
    let scope_library: Signal<Option<String>> = create_signal(None);
    let current_query: Signal<ChildQuery> = create_signal(browse::library_query(None));
    let loading = create_signal(true);
    let detail_loading = create_signal(matches!(route, MediaRoute::Item { .. }));
    let search_loading = create_signal(matches!(route, MediaRoute::Search { .. }));
    let refreshing = create_signal(false);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);
    let search_term = create_signal(match &route {
        MediaRoute::Search { words, .. } => words.clone(),
        _ => String::new(),
    });

    // Library filters, view mode and selection (roadmap 12d).
    let filter_genres: Signal<Vec<String>> = create_signal(Vec::new());
    let filter_years: Signal<Vec<i32>> = create_signal(Vec::new());
    let view_mode = create_signal(browse::ViewMode::from_stored(
        api::stored(browse::VIEW_MODE_KEY).as_deref(),
    ));
    let selecting = create_signal(false);
    let selected: Signal<Vec<String>> = create_signal(Vec::new());
    let bulk_busy = create_signal(false);

    let go = |r: MediaRoute| navigate(&r.path());

    // A page of a folder's, library's or person's titles.
    let load_items = move |parent_id: String,
                           query: ChildQuery,
                           start: i32,
                           person: Option<String>,
                           extra: String| {
        loading.set(true);
        spawn_local(async move {
            match api::get_items(
                &parent_id,
                query.item_types,
                start,
                PAGE_SIZE,
                query.sort_by,
                query.sort_order,
                query.recursive,
                person.as_deref(),
                &extra,
            )
            .await
            {
                Ok(result) => {
                    total_count.set(result.total_record_count);
                    start_index.set(result.start_index);
                    items.set(result.items);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };

    // The trail to an item, from its ancestors; the library (if any) is the
    // search scope.
    let set_trail = move |ancestors: Vec<BaseItemDto>, current: String| {
        scope_library.set(
            ancestors
                .iter()
                .find(|a| a.item_type == "CollectionFolder")
                .map(|a| a.id.clone()),
        );
        let steps: Vec<(String, String, String)> = ancestors
            .into_iter()
            .map(|a| (a.id, a.item_type, a.name))
            .collect();
        breadcrumbs.set(media_route::trail(&steps, &current));
    };

    // Load what the address names.
    let route_for_load = route.clone();
    spawn_local(async move {
        if let Err(e) = api::ensure_authenticated().await {
            error.set(Some(e.to_string()));
            loading.set(false);
            return;
        }
        let libs = match api::get_user_views().await {
            Ok(result) => result.items,
            Err(e) => {
                error.set(Some(e.to_string()));
                Vec::new()
            }
        };
        libraries.set(libs.clone());
        match route_for_load {
            MediaRoute::Libraries => loading.set(false),
            MediaRoute::Library {
                id,
                sort,
                filter,
                start,
            } => {
                let Some(lib) = libs.iter().find(|l| l.id == id) else {
                    error.set(Some("That library is not available.".to_string()));
                    loading.set(false);
                    return;
                };
                let mut query = browse::library_query(lib.collection_type.as_deref());
                if let Some(label) = sort.as_deref() {
                    query = browse::with_sort(query, label);
                }
                current_query.set(query);
                scope_library.set(Some(id.clone()));
                breadcrumbs.set(vec![Crumb {
                    label: lib.name.clone(),
                    path: None,
                }]);
                if let Ok(f) = api::get_filters(&id).await {
                    filter_genres.set(f.genres);
                    let mut years = f.years;
                    years.sort_unstable_by(|a, b| b.cmp(a));
                    filter_years.set(years);
                }
                load_items(id, query, start, None, filter.query());
            }
            MediaRoute::Folder { id, start } => match api::get_item(&id).await {
                Ok(item) => {
                    let query = browse::children_query(&item.item_type);
                    current_query.set(query);
                    let ancestors = api::get_ancestors(&id).await.unwrap_or_default();
                    set_trail(ancestors, item.name.clone());
                    load_items(id, query, start, None, String::new());
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    loading.set(false);
                }
            },
            MediaRoute::Person { id, name, start } => {
                let query = browse::person_query();
                current_query.set(query);
                breadcrumbs.set(vec![Crumb {
                    label: name,
                    path: None,
                }]);
                load_items(String::new(), query, start, Some(id), String::new());
            }
            MediaRoute::Item { id } => {
                loading.set(false);
                match api::get_item(&id).await {
                    Ok(item) => {
                        // Albums list their tracks, collections their films.
                        if let Some(sort) = browse::detail_children_sort(&item.item_type)
                            && let Ok(result) = api::get_items(
                                &item.id,
                                None,
                                0,
                                500,
                                sort,
                                "Ascending",
                                false,
                                None,
                                "",
                            )
                            .await
                        {
                            detail_tracks.set(result.items);
                        }
                        if item.item_type == "Movie"
                            && let Ok(result) = api::get_item_collections(&item.id).await
                        {
                            detail_collections.set(result.items);
                        }
                        if matches!(item.item_type.as_str(), "Movie" | "Series" | "Season")
                            && let Ok(extras) = api::get_extras(&item.id).await
                        {
                            detail_extras.set(extras);
                        }
                        let label =
                            browse::card_label(&item.item_type, &item.name, item.index_number);
                        let ancestors = api::get_ancestors(&id).await.unwrap_or_default();
                        set_trail(ancestors, label);
                        detail_item.set(Some(item));
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                detail_loading.set(false);
            }
            MediaRoute::Search {
                words,
                library,
                kind,
            } => {
                loading.set(false);
                let mut crumbs = Vec::new();
                if let Some(lib) = library
                    .as_deref()
                    .and_then(|id| libs.iter().find(|l| l.id == id))
                {
                    crumbs.push(Crumb {
                        label: lib.name.clone(),
                        path: Some(
                            MediaRoute::Library {
                                id: lib.id.clone(),
                                sort: None,
                                filter: Default::default(),
                                start: 0,
                            }
                            .path(),
                        ),
                    });
                }
                crumbs.push(Crumb {
                    label: browse::search_crumb_label(&words),
                    path: None,
                });
                breadcrumbs.set(crumbs);
                scope_library.set(library.clone());
                match api::search_hints(&words, library.as_deref(), kind.as_deref()).await {
                    Ok(result) => search_results.set(result.search_hints),
                    Err(e) => error.set(Some(e.to_string())),
                }
                search_loading.set(false);
            }
        }
    });

    // ── Actions: each goes to an address ────────────────

    let select_library = move |lib: BaseItemDto| {
        go(MediaRoute::Library {
            id: lib.id,
            sort: None,
            filter: Default::default(),
            start: 0,
        })
    };
    let browse_into = move |item: BaseItemDto| {
        go(MediaRoute::Folder {
            id: item.id,
            start: 0,
        })
    };
    let view_detail = move |item_id: String| go(MediaRoute::Item { id: item_id });
    let view_person = move |id: String, name: String| go(MediaRoute::Person { id, name, start: 0 });
    let here = create_signal(route.clone());
    let change_sort = move |label: String| {
        if let MediaRoute::Library { id, filter, .. } = here.get_clone() {
            go(MediaRoute::Library {
                id,
                sort: Some(label),
                filter,
                start: 0,
            });
        }
    };
    let go_to_start = move |n: i32| go(here.with(|r| r.with_start(n)));
    // A new filter: back to the first page, same order.
    let change_filter = move |edit: &dyn Fn(&mut media_route::LibraryFilter)| {
        if let MediaRoute::Library {
            id,
            sort,
            mut filter,
            ..
        } = here.get_clone()
        {
            edit(&mut filter);
            go(MediaRoute::Library {
                id,
                sort,
                filter,
                start: 0,
            });
        }
    };
    let set_view_mode = move |mode: browse::ViewMode| {
        api::store(browse::VIEW_MODE_KEY, mode.stored());
        view_mode.set(mode);
    };
    // Mark the selected titles played/unplayed, or refresh their metadata.
    let bulk = move |action: &'static str| {
        let ids = selected.get_clone();
        if ids.is_empty() || bulk_busy.get() {
            return;
        }
        bulk_busy.set(true);
        // Optimistic (roadmap 12g): ticks change at once; failures are put back.
        let marks = match action {
            "played" => Some(true),
            "unplayed" => Some(false),
            _ => None,
        };
        let set_played = move |which: &[String], played: bool| {
            items.update(|list| {
                for item in list.iter_mut().filter(|i| which.contains(&i.id)) {
                    item.user_data.get_or_insert_with(Default::default).played = played;
                }
            });
        };
        if let Some(played) = marks {
            set_played(&ids, played);
        }
        spawn_local(async move {
            let mut failed_ids: Vec<String> = Vec::new();
            for id in &ids {
                let r = match action {
                    "played" => api::set_played(id, true).await,
                    "unplayed" => api::set_played(id, false).await,
                    _ => api::refresh_item(id).await,
                };
                if r.is_err() {
                    failed_ids.push(id.clone());
                }
            }
            if let Some(played) = marks
                && !failed_ids.is_empty()
            {
                set_played(&failed_ids, !played);
            }
            let failed = failed_ids.len();
            let done = ids.len() - failed;
            let what = match action {
                "played" => "marked played",
                "unplayed" => "marked unplayed",
                _ => "queued for a metadata refresh",
            };
            toast.set(Some(if failed == 0 {
                format!("{done} {what}")
            } else {
                format!("{done} {what}; {failed} failed")
            }));
            selected.set(Vec::new());
            selecting.set(false);
            bulk_busy.set(false);
        });
    };
    let back_to_libraries = move |_| go(MediaRoute::Libraries);
    let go_back = move |_| {
        if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = history.back();
        }
    };

    let do_search = move || {
        let words = search_term.get_clone();
        if words.trim().is_empty() {
            return;
        }
        go(MediaRoute::Search {
            words: words.trim().to_string(),
            library: scope_library.get_clone(),
            kind: here.with(|r| match r {
                MediaRoute::Search { kind, .. } => kind.clone(),
                _ => None,
            }),
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
                    Err(e) => error.set(Some(e.to_string())),
                }
                refreshing.set(false);
            }
        });
    };
    let sortable = matches!(route, MediaRoute::Library { .. });

    // ── Render ──────────────────────────────────────────

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                crate::timer::clear_toast_later(toast, msg.clone());
                view! {
                    div(class="toast-container", role="status", aria-live="polite") {
                        div(class="toast toast-success") { (msg) }
                    }
                }
            } else {
                view! {}
            })

            // Error
            ErrorNotice(error=error)

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
                            list=move || breadcrumbs.get_clone(),
                            view=move |crumb: Crumb| {
                                let label = crumb.label.clone();
                                match crumb.path.clone() {
                                    None => view! {
                                        span(class="breadcrumb-sep") { " / " }
                                        span(class="breadcrumb-current") { (label) }
                                    },
                                    Some(path) => {
                                        let href = path.clone();
                                        view! {
                                        span(class="breadcrumb-sep") { " / " }
                                        a(class="breadcrumb-link", href=href, on:click=move |e: web_sys::MouseEvent| {
                                            e.prevent_default();
                                            navigate(&path);
                                        }) { (label) }
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
                        view! { Loading(kind="grid") }
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
                        view! { Loading(kind="grid") }
                    } else {
                        let item_list = items.get_clone();
                        let total = total_count.get();
                        let start = start_index.get();
                        // A library's top level can be re-ordered.
                        let sortable = sortable && current_query.get().recursive;
                        let sort_select = if sortable {
                            let selected_sort = browse::sort_label(current_query.get());
                            view! {
                                label(class="grid-sort") {
                                    "Sort by "
                                    select(on:change=move |e: web_sys::Event| {
                                        let value = e
                                            .target()
                                            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok())
                                            .map(|s| s.value())
                                            .unwrap_or_default();
                                        change_sort(value);
                                    }) {
                                        Indexed(
                                            list=move || browse::SORT_OPTIONS.to_vec(),
                                            view=move |o: browse::SortOption| {
                                                let is_selected = o.label == selected_sort;
                                                view! { option(value=o.label, selected=is_selected) { (o.label) } }
                                            }
                                        )
                                    }
                                }
                            }
                        } else {
                            view! {}
                        };
                        // Filters by genre, year and rating: films and shows at a
                        // library's top level (artists have no year or rating).
                        let filterable = sortable
                            && matches!(current_query.get().item_types, Some("Movie") | Some("Series"));
                        let filtered = here.with(|r| matches!(r, MediaRoute::Library { filter, .. } if !filter.is_empty()));
                        let filters = if filterable {
                            let current = here.with(|r| match r {
                                MediaRoute::Library { filter, .. } => filter.clone(),
                                _ => Default::default(),
                            });
                            let genres = filter_genres.get_clone();
                            let years = filter_years.get_clone();
                            let cur_genre = current.genre.clone().unwrap_or_default();
                            let cur_year = current.year.map(|y| y.to_string()).unwrap_or_default();
                            let cur_rating = current.min_rating.map(|r| r.to_string()).unwrap_or_default();
                            let any_filter = !current.is_empty();
                            view! {
                                select(aria-label="Genre", on:change=move |e: web_sys::Event| {
                                    let v = select_value(e);
                                    change_filter(&|f| f.genre = Some(v.clone()).filter(|g| !g.is_empty()));
                                }) {
                                    option(value="") { "All genres" }
                                    Indexed(
                                        list=move || genres.clone(),
                                        view=move |g: String| {
                                            let is_selected = g == cur_genre;
                                            let label = g.clone();
                                            view! { option(value=g, selected=is_selected) { (label) } }
                                        }
                                    )
                                }
                                select(aria-label="Year", on:change=move |e: web_sys::Event| {
                                    let v = select_value(e);
                                    change_filter(&|f| f.year = v.parse().ok());
                                }) {
                                    option(value="") { "Any year" }
                                    Indexed(
                                        list=move || years.clone(),
                                        view=move |y: i32| {
                                            let value = y.to_string();
                                            let is_selected = value == cur_year;
                                            view! { option(value=value, selected=is_selected) { (y) } }
                                        }
                                    )
                                }
                                select(aria-label="Rating", on:change=move |e: web_sys::Event| {
                                    let v = select_value(e);
                                    change_filter(&|f| f.min_rating = v.parse().ok());
                                }) {
                                    option(value="") { "Any rating" }
                                    Indexed(
                                        list=move || vec![5u8, 6, 7, 8, 9],
                                        view=move |r: u8| {
                                            let value = r.to_string();
                                            let is_selected = value == cur_rating;
                                            view! { option(value=value, selected=is_selected) { (r) "+" } }
                                        }
                                    )
                                }
                                (if any_filter {
                                    view! {
                                        button(class="btn btn-secondary btn-sm", on:click=move |_| change_filter(&|f| *f = Default::default())) { "Clear filters" }
                                    }
                                } else {
                                    view! {}
                                })
                            }
                        } else {
                            view! {}
                        };
                        let is_list = view_mode.get() == browse::ViewMode::List;
                        let sort_bar = view! {
                            div(class="grid-toolbar") {
                                (sort_select)
                                (filters)
                                div(class="grid-toolbar-end") {
                                    button(
                                        class=if is_list { "btn btn-secondary btn-sm" } else { "btn btn-primary btn-sm" },
                                        aria-pressed=(!is_list).to_string(),
                                        on:click=move |_| set_view_mode(browse::ViewMode::Grid)
                                    ) { "Grid" }
                                    button(
                                        class=if is_list { "btn btn-primary btn-sm" } else { "btn btn-secondary btn-sm" },
                                        aria-pressed=is_list.to_string(),
                                        on:click=move |_| set_view_mode(browse::ViewMode::List)
                                    ) { "List" }
                                    button(class="btn btn-secondary btn-sm", on:click=move |_| {
                                        selecting.set(!selecting.get());
                                        selected.set(Vec::new());
                                    }) { (if selecting.get() { "Cancel" } else { "Select" }) }
                                }
                            }
                            (if selecting.get() {
                                let count = selected.with(Vec::len);
                                let none = count == 0 || bulk_busy.get();
                                view! {
                                    div(class="bulk-bar", aria-live="polite") {
                                        span { (count) " selected" }
                                        button(class="btn btn-secondary btn-sm", disabled=none, on:click=move |_| bulk("played")) { "Mark played" }
                                        button(class="btn btn-secondary btn-sm", disabled=none, on:click=move |_| bulk("unplayed")) { "Mark unplayed" }
                                        button(class="btn btn-secondary btn-sm", disabled=none, on:click=move |_| bulk("refresh")) { "Refresh metadata" }
                                    }
                                }
                            } else {
                                view! {}
                            })
                        };
                        if item_list.is_empty() {
                            // The toolbar stays, so a filter that matches nothing can be cleared.
                            let message = if filtered {
                                "Nothing matches these filters."
                            } else {
                                "No items found."
                            };
                            view! {
                                div {
                                    (sort_bar)
                                    div(class="empty-state") { (message) }
                                }
                            }
                        } else {
                            view! {
                                div {
                                    (sort_bar)
                                    div(class=if is_list { "media-grid list" } else { "media-grid" }) {
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
                                                let user = item.user_data.clone().unwrap_or_default();
                                                let badge = browse::card_badge(is_folder, user.played, user.unplayed_item_count);
                                                let progress = browse::progress_percent(
                                                    user.played_percentage,
                                                    user.playback_position_ticks,
                                                    item.runtime_ticks,
                                                    user.played,
                                                );
                                                let display_name =
                                                    browse::card_label(&item_type, &item_name, item.index_number);

                                                let (poster_class, placeholder_class) = browse::grid_classes(&item_type);

                                                let opens_detail = browse::opens_detail(&item_type, is_folder);
                                                let select_id = item.id.clone();
                                                let check_id = item.id.clone();
                                                let card_class = move || {
                                                    if selecting.get() && selected.with(|s| s.contains(&check_id)) {
                                                        "media-card selected"
                                                    } else {
                                                        "media-card"
                                                    }
                                                };
                                                let on_click = move |_| {
                                                    if selecting.get() {
                                                        selected.update(|s| browse::toggle_selected(s, &select_id));
                                                    } else if opens_detail {
                                                        view_detail(item_id.clone());
                                                    } else {
                                                        browse_into(item_for_click.clone());
                                                    }
                                                };

                                                view! {
                                                    div(class=card_class, on:click=on_click) {
                                                        div(class="media-card-art") {
                                                        (if has_image {
                                                            let url = img_url.clone();
                                                            view! { img(class=poster_class, src=url, alt="", loading="lazy") }
                                                        } else {
                                                            let icon = icon_for_item_type(&item_type);
                                                            view! { div(class=placeholder_class) { (icon) } }
                                                        })
                                                        (match badge {
                                                            Some(b) => {
                                                                let text = b.text();
                                                                view! { span(class=b.class()) { (text) } }
                                                            }
                                                            None => view! {},
                                                        })
                                                        (match progress {
                                                            Some(pct) => {
                                                                let style = format!("width: {pct:.1}%;");
                                                                view! { div(class="card-progress") { div(class="card-progress-fill", style=style) {} } }
                                                            }
                                                            None => view! {},
                                                        })
                                                        }
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
                                        view! {
                                            div(class="pagination") {
                                                button(
                                                    class="btn btn-secondary btn-sm",
                                                    disabled=!has_prev,
                                                    on:click=move |_| go_to_start((start - PAGE_SIZE).max(0))
                                                ) { "Previous" }
                                                span(class="pagination-info") {
                                                    (start + 1) " - " (showing_end) " of " (total)
                                                }
                                                button(
                                                    class="btn btn-secondary btn-sm",
                                                    disabled=!has_next,
                                                    on:click=move |_| go_to_start(start + PAGE_SIZE)
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
                        view! { Loading(kind="detail") }
                    } else if let Some(item) = detail_item.get_clone() {
                        let has_image = has_primary_image(&item);
                        let img_url = primary_image_url(&item.id, primary_tag(&item).as_deref(), browse::DETAIL_IMAGE_WIDTH);
                        let refresh_id = detail_id.clone();
                        let detail_item_type = item.item_type.clone();
                        let is_album = detail_item_type == "MusicAlbum";
                        let is_collection = detail_item_type == "BoxSet";
                        let (detail_poster_class, detail_placeholder_class) =
                            browse::detail_classes(&detail_item_type);

                        view! {
                            div(class="detail-page") {
                                div(class="detail-header") {
                                    div(class=detail_poster_class) {
                                        (if has_image {
                                            let url = img_url.clone();
                                            view! { img(src=url, alt="", decoding="async") }
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
                                                on:click=go_back
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

                                // The collections a film is in
                                ({
                                    let sets = detail_collections.get_clone();
                                    if sets.is_empty() {
                                        view! {}
                                    } else {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "Collections" }
                                                div(class="provider-list") {
                                                    Indexed(
                                                        list=move || sets.clone(),
                                                        view=move |set: BaseItemDto| {
                                                            let set_id = set.id.clone();
                                                            let name = set.name.clone();
                                                            view! {
                                                                button(
                                                                    class="breadcrumb-link",
                                                                    on:click=move |_| view_detail(set_id.clone())
                                                                ) { (name) }
                                                            }
                                                        }
                                                    )
                                                }
                                            }
                                        }
                                    }
                                })

                                // Extras (trailers, featurettes, ...)
                                ({
                                    let extras = detail_extras.get_clone();
                                    if extras.is_empty() {
                                        view! {}
                                    } else {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "Extras" }
                                                div(class="provider-list") {
                                                    Indexed(
                                                        list=move || extras.clone(),
                                                        view=move |extra: BaseItemDto| {
                                                            let extra_id = extra.id.clone();
                                                            let text = format!(
                                                                "{} · {}",
                                                                extra.name,
                                                                browse::extra_label(extra.extra_type.as_deref())
                                                            );
                                                            view! {
                                                                button(
                                                                    class="breadcrumb-link",
                                                                    on:click=move |_| view_detail(extra_id.clone())
                                                                ) { (text) }
                                                            }
                                                        }
                                                    )
                                                }
                                            }
                                        }
                                    }
                                })

                                // Films (for collections)
                                (if is_collection {
                                    let films = detail_tracks.get_clone();
                                    if films.is_empty() {
                                        view! {}
                                    } else {
                                        view! {
                                            div(class="detail-section") {
                                                h3 { "Films" }
                                                div(class="media-grid") {
                                                    Indexed(
                                                        list=move || films.clone(),
                                                        view=move |film: BaseItemDto| {
                                                            let film_id = film.id.clone();
                                                            let name = film.name.clone();
                                                            let year = film.production_year;
                                                            let has_image = has_primary_image(&film);
                                                            let url = primary_image_url(&film.id, primary_tag(&film).as_deref(), browse::CARD_IMAGE_WIDTH);
                                                            let (poster_class, placeholder_class) = browse::grid_classes(&film.item_type);
                                                            let icon = icon_for_item_type(&film.item_type);
                                                            view! {
                                                                div(class="media-card", on:click=move |_| view_detail(film_id.clone())) {
                                                                    (if has_image {
                                                                        let url = url.clone();
                                                                        view! { img(class=poster_class, src=url, alt="", loading="lazy") }
                                                                    } else {
                                                                        view! { div(class=placeholder_class) { (icon) } }
                                                                    })
                                                                    div(class="media-card-title") { (name) }
                                                                    (if let Some(y) = year {
                                                                        view! { div(class="media-card-year") { (y) } }
                                                                    } else {
                                                                        view! {}
                                                                    })
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
                                                        let person_name = name.clone();
                                                        let label_text = name;
                                                        let person_id = person.id.clone();
                                                        let card_class = if person_id.is_some() {
                                                            "person-card person-card-link"
                                                        } else {
                                                            "person-card"
                                                        };
                                                        let open_person = move |_| {
                                                            if let Some(id) = person_id.clone() {
                                                                view_person(id, person_name.clone());
                                                            }
                                                        };
                                                        view! {
                                                            div(class=card_class, on:click=open_person) {
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
                    let current_kind = here.with(|r| match r {
                        MediaRoute::Search { kind, .. } => kind.clone().unwrap_or_default(),
                        _ => String::new(),
                    });
                    let kind_bar = view! {
                        div(class="grid-toolbar") {
                            label(class="grid-sort") {
                                "Show "
                                select(on:change=move |e: web_sys::Event| {
                                    let value = select_value(e);
                                    if let MediaRoute::Search { words, library, .. } = here.get_clone() {
                                        go(MediaRoute::Search {
                                            words,
                                            library,
                                            kind: Some(value).filter(|k| !k.is_empty()),
                                        });
                                    }
                                }) {
                                    Indexed(
                                        list=move || browse::SEARCH_KINDS.to_vec(),
                                        view=move |(value, label): (&'static str, &'static str)| {
                                            let is_selected = value == current_kind;
                                            view! { option(value=value, selected=is_selected) { (label) } }
                                        }
                                    )
                                }
                            }
                        }
                    };
                    let body = if search_loading.get() {
                        view! { Loading(kind="rows") }
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
                                            // Worked out here: the view's closures cannot move item_type.
                                            let thumb_class = browse::search_thumb_class(&item_type);
                                            let icon = icon_for_item_type(&item_type);

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
                                                        view! { img(class=thumb_class, src=url, alt="", loading="lazy") }
                                                    } else {
                                                        let class = format!("{thumb_class} media-poster-placeholder");
                                                        view! { div(class=class) { (icon) } }
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
                    };
                    view! { div { (kind_bar) (body) } }
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
