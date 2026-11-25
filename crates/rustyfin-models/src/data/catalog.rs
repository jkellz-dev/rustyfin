use uuid::Uuid;

/// One changed item.
#[derive(Debug, Clone)]
pub struct CatalogChange {
    pub id: Uuid,
    pub item_type: String,
    pub library_id: Uuid,
    /// Its change, for paging (`xid,id`).
    pub change_xid: String,
}
