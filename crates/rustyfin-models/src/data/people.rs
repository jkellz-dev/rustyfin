use uuid::Uuid;

/// A person (actor, director, writer, ...).
#[derive(Debug, Clone)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub person_type: String,
    pub overview: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}

/// A person in an item's cast or crew: their role and billing order.
#[derive(Debug, Clone)]
pub struct MediaItemPerson {
    pub item_id: Uuid,
    pub person_id: Uuid,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
}
