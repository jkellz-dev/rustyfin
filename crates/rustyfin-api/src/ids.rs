//! Ids in answers as 32 lowercase hex digits, no dashes: how this API's
//! callers have always read them.

use serde::Serializer;
use uuid::Uuid;

/// `#[serde(serialize_with = "ids::simple")]`
pub fn simple<S: Serializer>(id: &Uuid, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&id.simple())
}

#[cfg(test)]
mod tests {
    #[derive(serde::Serialize)]
    struct Row {
        #[serde(serialize_with = "super::simple")]
        id: uuid::Uuid,
    }

    #[test]
    fn ids_are_written_without_dashes() {
        let id = uuid::Uuid::parse_str("0f8fad5b-d9cb-469f-a165-70867728950e").unwrap();
        assert_eq!(
            serde_json::to_string(&Row { id }).unwrap(),
            r#"{"id":"0f8fad5bd9cb469fa16570867728950e"}"#
        );
    }
}
