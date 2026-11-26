//! The dashboard's health panel, in Jellyfin's shape: what
//! `rustyfin_services::health` reports, PascalCase.

use rustyfin_models::data::{Data, Library};
use rustyfin_models::dto::{
    CacheHealth, DatabaseHealth, FolderHealth, LibraryHealth, SearchHealth, SystemHealth,
};
use rustyfin_services::health as h;

use crate::router::AppState;

/// The whole report.
pub async fn system_health<D: Data>(state: &AppState<D>, libs: &[Library]) -> SystemHealth {
    to_dto(
        h::system_health(
            &state.data,
            libs,
            &state.image_cache_dir,
            state.people_cache_dir.as_deref(),
        )
        .await,
    )
}

fn to_dto(r: h::SystemHealth) -> SystemHealth {
    SystemHealth {
        database: DatabaseHealth {
            ok: r.database.ok,
            latency_ms: r.database.latency_ms,
            pool_size: r.database.pool_size,
            pool_idle: r.database.pool_idle,
            error: r.database.error,
        },
        search: SearchHealth {
            backend: r.search.backend,
            ok: r.search.ok,
            detail: r.search.detail,
        },
        libraries: r
            .libraries
            .into_iter()
            .map(|l| LibraryHealth {
                name: l.name,
                status: l.status,
                folders: l
                    .folders
                    .into_iter()
                    .map(|f| FolderHealth {
                        path: f.path,
                        status: f.status,
                    })
                    .collect(),
            })
            .collect(),
        caches: r
            .caches
            .into_iter()
            .map(|c| CacheHealth {
                name: c.name,
                path: c.path,
                status: c.status,
                size_bytes: c.size_bytes,
                file_count: c.file_count,
            })
            .collect(),
    }
}
