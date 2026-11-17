mod a11y;
mod api;
mod api_error;
mod browse;
mod components;
mod dashboard_view;
mod form_rules;
mod media_route;
mod pages;
#[allow(dead_code)]
mod player_logic;
mod request_state;
mod timer;
mod types;
mod user_view;

use sycamore::prelude::*;
use sycamore_router::{HistoryIntegration, Route, Router};

use components::nav::Sidebar;
use pages::{
    dashboard::Dashboard, libraries::Libraries, media::Media, setup::Setup, tasks::Tasks,
    user_detail::UserDetail, users::Users,
};

#[derive(Route, Clone)]
enum AppRoutes {
    #[to("/web")]
    Dashboard,
    #[to("/web/index.html")]
    DashboardIndex,
    #[to("/web/libraries")]
    Libraries,
    /// Every place in the media browser has an address (media_route).
    #[to("/web/media/<path..>")]
    Media { path: Vec<String> },
    #[to("/web/users")]
    Users,
    #[to("/web/users/<id>")]
    UserDetail { id: String },
    #[to("/web/tasks")]
    Tasks,
    #[to("/web/setup")]
    Setup,
    #[not_found]
    NotFound,
}

fn main() {
    a11y::install();
    sycamore::render(|| {
        view! {
            Router(
                integration=HistoryIntegration::new(),
                view=move |route: ReadSignal<AppRoutes>| {
                    // Static root node — Router's click handler attaches here and persists
                    view! {
                        div(class="app-layout") {
                            a(class="skip-link", href="#main") { "Skip to content" }
                            div(id="sr-announcer", class="sr-only", aria-live="polite") {}
                            Sidebar {}
                            main(class="main-content", id="main", tabindex="-1") {
                                // Only this dynamic block rebuilds on route changes
                                ({
                                    let path = web_sys::window()
                                        .and_then(|w| w.location().pathname().ok())
                                        .unwrap_or_default();
                                    a11y::announce_page(&path);
                                    let current = route.get_clone();
                                    match current {
                                    AppRoutes::Setup => view! { Setup {} },
                                    AppRoutes::Libraries => view! { Libraries {} },
                                    AppRoutes::Media { path } => view! { Media(path=format!("/web/media/{}", path.join("/"))) },
                                    AppRoutes::Users => view! { Users {} },
                                    AppRoutes::UserDetail { id } => view! { UserDetail(user_id=id) },
                                    AppRoutes::Tasks => view! { Tasks {} },
                                    _ => view! { Dashboard {} },
                                    }
                                })
                            }
                        }
                    }
                }
            )
        }
    });
}
