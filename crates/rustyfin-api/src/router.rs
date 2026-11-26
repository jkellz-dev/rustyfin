//! The routes, and what runs before them: the path in the case the router
//! knows ([`crate::paths`]), who is asking ([`crate::caller`]), and which
//! view of the data a linked server gets ([`scope`]).

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use rustyfin_models::data::Data;
use tower::ServiceExt;

use crate::paths::PREFIX;
use crate::routes::{catalog, links, permissions};
use crate::state::ApiState;

/// The API's router. A server linking this one is served by a second copy
/// of the routes over `Data::local_only()` (`rustyfin_services::link::scope`).
///
/// `observe` is what the server does to every route (metrics, the request
/// log).
pub fn build_router<D: Data>(state: ApiState<D>, observe: fn(Router) -> Router) -> Router {
    let local_data = state.data.local_only();
    let local = ApiState {
        services: state.services.with_data(local_data.clone()),
        data: local_data,
        ..state.clone()
    };
    let linking = Scope {
        data: state.data.clone(),
        local_routes: routes(local, observe),
    };
    let identify = state.clone();
    let routes = routes(state, observe);
    Router::new().fallback_service(
        tower::ServiceBuilder::new()
            .layer(axum::middleware::from_fn(crate::paths::lower_case))
            .layer(axum::middleware::from_fn_with_state(
                identify,
                crate::caller::identify::<D>,
            ))
            .layer(axum::middleware::from_fn_with_state(linking, scope::<D>))
            .service(routes),
    )
}

fn routes<D: Data>(state: ApiState<D>, observe: fn(Router) -> Router) -> Router {
    let api = Router::new()
        // Linked servers, for the admin (roadmap 16b).
        .route("/links", get(links::list::<D>))
        .route("/links/{id}/sync", post(links::sync_now::<D>))
        // The change feed a linked Rustyfin syncs from (roadmap 16b.2r).
        .route("/catalog", get(catalog::catalog::<D>))
        // What a user may do beyond what the Jellyfin API sets.
        .route("/users/permissions", get(permissions::list::<D>))
        .route(
            "/users/{id}/permissions",
            get(permissions::get::<D>).patch(permissions::patch::<D>),
        );
    observe(Router::new().nest(PREFIX, api).with_state(state))
}

/// What [`scope`] needs: the whole data layer to look tokens up, and the
/// routes for linking servers.
#[derive(Clone)]
struct Scope<D> {
    data: D,
    local_routes: Router,
}

/// Middleware before routing: a linking server's request goes to the
/// local-only routes.
async fn scope<D: Data>(State(scope): State<Scope<D>>, req: Request, next: Next) -> Response {
    let token = crate::caller::request_token(req.headers(), req.uri().query());
    if let Some(token) = token
        && rustyfin_services::link::scope::is_linking(&scope.data, &token).await
    {
        return match scope.local_routes.clone().oneshot(req).await {
            Ok(response) => response,
            Err(never) => match never {},
        }
        .into_response();
    }
    next.run(req).await
}
