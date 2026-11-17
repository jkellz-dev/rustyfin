//! Checks a form makes before it is sent (roadmap 12c), so mistakes are
//! named next to the field instead of coming back as a server error.

/// A library as the check sees it: its id, name and folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFacts<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub paths: &'a [String],
}

/// What is wrong with a library form, one sentence per problem, in field
/// order. `editing` is the id of the library being edited (its own name and
/// folders are not duplicates). Empty folder rows are ignored.
pub fn library_problems(
    name: &str,
    paths: &[String],
    others: &[LibraryFacts<'_>],
    editing: Option<&str>,
) -> Vec<String> {
    let _ = (name, paths, others, editing);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_good_library_has_no_problems() {
        let movies = paths(&["/data/media/movies"]);
        let others = [LibraryFacts {
            id: "1",
            name: "Movies",
            paths: &movies,
        }];
        assert!(
            library_problems("Music", &paths(&["/data/media/music", ""]), &others, None).is_empty()
        );
        // Editing a library keeps its own name and folders.
        assert!(library_problems("Movies", &movies, &others, Some("1")).is_empty());
    }

    #[test]
    fn names_and_folders_are_checked() {
        let movies = paths(&["/data/media/movies"]);
        let others = [LibraryFacts {
            id: "1",
            name: "Movies",
            paths: &movies,
        }];
        assert_eq!(
            library_problems("  ", &paths(&[""]), &others, None),
            vec!["Give the library a name.", "Add at least one folder."]
        );
        assert_eq!(
            library_problems(" movies ", &paths(&["/data/media/films"]), &others, None),
            vec!["There is already a library named \"Movies\"."]
        );
        assert_eq!(
            library_problems("Films", &paths(&["media/films"]), &others, None),
            vec!["\"media/films\" is not a full path (it should start with /)."]
        );
        assert_eq!(
            library_problems("Films", &paths(&["/a", "/a/"]), &others, None),
            vec!["\"/a/\" is listed twice."]
        );
        assert_eq!(
            library_problems("Films", &paths(&["/data/media/movies/"]), &others, None),
            vec!["\"/data/media/movies/\" is already in \"Movies\"."]
        );
    }
}
