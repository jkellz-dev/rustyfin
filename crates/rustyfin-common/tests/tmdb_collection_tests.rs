//! The collection TMDB puts a film in (`belongs_to_collection` of
//! `/movie/{id}`), for `rustyfin artwork --check-collections` (owner,
//! 2026-11-16).
use rustyfin_common::tmdb::parse_movie_collection;

#[test]
fn a_films_collection_is_read() {
    let json = r#"{"id":170,"title":"28 Days Later","belongs_to_collection":
        {"id":1565,"name":"28 Days/Weeks/Years Later Collection","poster_path":"/p.jpg"}}"#;
    assert_eq!(
        parse_movie_collection(json),
        Some(Some((
            1565,
            "28 Days/Weeks/Years Later Collection".to_string()
        )))
    );
}

#[test]
fn a_film_in_no_collection_is_none_and_an_error_is_unknown() {
    assert_eq!(
        parse_movie_collection(r#"{"id":1,"belongs_to_collection":null}"#),
        Some(None)
    );
    assert_eq!(parse_movie_collection(r#"{"id":1}"#), Some(None));
    // TMDB's error body (bad key, unknown id): not an answer.
    assert_eq!(
        parse_movie_collection(r#"{"success":false,"status_code":34}"#),
        None
    );
    assert_eq!(parse_movie_collection("<html>"), None);
}
