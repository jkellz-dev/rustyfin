-- Per-user library access control.
-- When enable_all_libraries is true (default), the user sees all libraries.
-- When false, only libraries listed in user_library_access are visible.

ALTER TABLE users ADD COLUMN enable_all_libraries BOOLEAN NOT NULL DEFAULT true;

CREATE TABLE user_library_access (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, library_id)
);
