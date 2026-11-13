mod api;
mod components;
mod pages;
mod types;

use sycamore::prelude::*;
use sycamore_router::{HistoryIntegration, Route, Router};

use components::nav::Sidebar;
use pages::{
    dashboard::Dashboard, libraries::Libraries, media::Media, setup::Setup, tasks::Tasks,
    users::Users,
};

#[derive(Route, Clone)]
enum AppRoutes {
    #[to("/web")]
    Dashboard,
    #[to("/web/index.html")]
    DashboardIndex,
    #[to("/web/libraries")]
    Libraries,
    #[to("/web/media")]
    Media,
    #[to("/web/users")]
    Users,
    #[to("/web/tasks")]
    Tasks,
    #[to("/web/setup")]
    Setup,
    #[not_found]
    NotFound,
}

fn main() {
    sycamore::render(|| {
        view! {
            Router(
                integration=HistoryIntegration::new(),
                view=move |route: ReadSignal<AppRoutes>| {
                    // Static root node — Router's click handler attaches here and persists
                    view! {
                        div(class="app-layout") {
                            Sidebar {}
                            main(class="main-content") {
                                // Only this dynamic block rebuilds on route changes
                                (match route.get_clone() {
                                    AppRoutes::Setup => view! { Setup {} },
                                    AppRoutes::Libraries => view! { Libraries {} },
                                    AppRoutes::Media => view! { Media {} },
                                    AppRoutes::Users => view! { Users {} },
                                    AppRoutes::Tasks => view! { Tasks {} },
                                    _ => view! { Dashboard {} },
                                })
                            }
                        }
                    }
                }
            )
        }
    });
}
