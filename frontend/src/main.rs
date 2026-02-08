mod api;
mod components;
mod pages;
mod types;

use sycamore::prelude::*;
use sycamore_router::{HistoryIntegration, Route, Router};

use components::nav::Layout;
use pages::{dashboard::Dashboard, libraries::Libraries, setup::Setup, users::Users};

#[derive(Route)]
enum AppRoutes {
    #[to("/web")]
    Dashboard,
    #[to("/web/index.html")]
    DashboardIndex,
    #[to("/web/libraries")]
    Libraries,
    #[to("/web/users")]
    Users,
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
                    view! {
                        (match route.get_clone() {
                            AppRoutes::Setup => view! { Setup {} },
                            AppRoutes::Dashboard | AppRoutes::DashboardIndex | AppRoutes::NotFound => view! {
                                Layout {
                                    Dashboard {}
                                }
                            },
                            AppRoutes::Libraries => view! {
                                Layout {
                                    Libraries {}
                                }
                            },
                            AppRoutes::Users => view! {
                                Layout {
                                    Users {}
                                }
                            },
                        })
                    }
                }
            )
        }
    });
}
