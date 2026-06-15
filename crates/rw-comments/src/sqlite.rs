use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
    SqliteSynchronous,
};
use sqlx::{Row, query};
use uuid::Uuid;

use crate::error::StoreError;
use crate::model::{
    Author, Comment, CommentFilter, CommentStatus, CreateComment, Selector, UpdateComment,
};

/// One forward migration applied inside a single transaction.
///
/// All `stmts` must be idempotent (`IF NOT EXISTS`, probe-then-`ADD COLUMN`,
/// etc.) — the pre-framework upgrade path runs v1 against a DB that already
/// has the `comments` table.
struct Migration {
    version: i64,
    stmts: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        stmts: &[
            "CREATE TABLE IF NOT EXISTS comments (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                parent_id TEXT,
                body TEXT NOT NULL,
                selectors TEXT NOT NULL,
                author TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_comments_document_id ON comments (document_id)",
            "CREATE INDEX IF NOT EXISTS idx_comments_parent_id ON comments (parent_id)",
            "CREATE INDEX IF NOT EXISTS idx_comments_document_status
                ON comments (document_id, status)",
        ],
    },
    Migration {
        // Soft-delete signal: a nullable `deleted_at` timestamp column.
        version: 2,
        stmts: &["ALTER TABLE comments ADD COLUMN deleted_at TEXT"],
    },
];

// Derived from MIGRATIONS so tests stay in sync automatically.
// `#[cfg(test)]` only — production code derives `latest` from the `migrations`
// parameter of `migrate_with` so it works correctly for test-only slices too.
#[cfg(test)]
const LATEST_SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;

// Compile-time invariants on the MIGRATIONS slice:
//   1. Non-empty — the LATEST_SCHEMA_VERSION index expression would panic otherwise.
//   2. First version >= 1 — v=0 is silently skipped: the apply filter is
//      `m.version > current` and a fresh DB starts at `current = 0`.
//   3. Strictly monotonic — out-of-order versions mis-classify a legitimate DB
//      as IncompatibleSchema and apply migrations in slice order, not version order.
const _: () = {
    assert!(!MIGRATIONS.is_empty(), "MIGRATIONS must not be empty");
    assert!(
        MIGRATIONS[0].version >= 1,
        "MIGRATIONS[0].version must be >= 1 (v=0 would be silently skipped by the apply filter)",
    );
    let mut i = 1;
    while i < MIGRATIONS.len() {
        assert!(
            MIGRATIONS[i].version > MIGRATIONS[i - 1].version,
            "MIGRATIONS must be strictly monotonic in version",
        );
        i += 1;
    }
};

pub struct SqliteCommentStore {
    pool: SqlitePool,
}

impl SqliteCommentStore {
    /// Default on-disk location for the comment store relative to a project's
    /// `.rw/` directory — `<project_dir>/comments/sqlite.db`.
    #[must_use]
    pub fn default_path(project_dir: &Path) -> PathBuf {
        project_dir.join("comments").join("sqlite.db")
    }

    /// Opens (or creates) a `SQLite` database at `path` and runs migrations.
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;

        Self::migrate(&pool).await?;

        Ok(Self { pool })
    }

    /// Opens an in-memory `SQLite` database.
    ///
    /// Useful for tests and for ephemeral scratch stores that don't need to
    /// persist between runs. The pool is capped at one connection because
    /// in-memory `SQLite` databases are bound to the connection that created
    /// them — multiple connections would see independent databases.
    pub async fn open_memory() -> Result<Self, StoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        Self::migrate(&pool).await?;

        Ok(Self { pool })
    }

    async fn migrate(pool: &SqlitePool) -> Result<(), StoreError> {
        Self::migrate_with(pool, MIGRATIONS).await
    }

    async fn migrate_with(pool: &SqlitePool, migrations: &[Migration]) -> Result<(), StoreError> {
        // Do NOT call this while holding another transaction on a pool with
        // `max_connections = 1` (e.g. `open_memory()`): it would deadlock at
        // `pool.begin_with` — the call waits for the outer connection to be
        // released, which can't happen until the outer transaction commits.

        let latest = migrations.last().map_or(0, |m| m.version);

        // Created outside the BEGIN IMMEDIATE below so it's visible to a
        // concurrent migrator that arrives after this point and tries to read
        // MAX(version). SQLite serializes DDL, so two racing CREATE TABLE IF
        // NOT EXISTS calls are safe: one wins the schema-cookie bump, the
        // other no-ops. If the transaction below rolls back, the empty table
        // is harmless — the next call sees current = 0 and retries.
        query(
            "CREATE TABLE IF NOT EXISTS schema_versions (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await?;

        // BEGIN IMMEDIATE serializes concurrent migrators: the second waits
        // here until the first commits, then re-reads MAX and sees the
        // version already applied, skipping it cleanly. Without it, two
        // callers can both read MAX=0, both apply v1 DDL (idempotent), then
        // both INSERT version=1 → PRIMARY KEY collision.
        // The `Transaction` drop rolls back automatically on any error path.
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let current: i64 = query("SELECT COALESCE(MAX(version), 0) AS v FROM schema_versions")
            .fetch_one(&mut *tx)
            .await?
            .get("v");

        // Refuse to open a DB written by a newer binary: any future migration
        // we don't know about has already shipped data we may misread.
        if current > latest {
            return Err(StoreError::IncompatibleSchema {
                db: current,
                binary: latest,
            });
        }

        for migration in migrations.iter().filter(|m| m.version > current) {
            for stmt in migration.stmts {
                query(*stmt).execute(&mut *tx).await?;
            }
            query("INSERT INTO schema_versions (version, applied_at) VALUES (?, ?)")
                .bind(migration.version)
                .bind(Utc::now().to_rfc3339())
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    fn row_to_comment(row: &SqliteRow) -> Result<Comment, StoreError> {
        let id_str: String = row.get("id");
        let id: Uuid = id_str.parse()?;

        let parent_id_str: Option<String> = row.get("parent_id");
        let parent_id = parent_id_str.map(|s| s.parse::<Uuid>()).transpose()?;

        let selectors_json: String = row.get("selectors");
        let selectors: Vec<Selector> = serde_json::from_str(&selectors_json)?;

        let author_json: String = row.get("author");
        let author: Author = serde_json::from_str(&author_json)?;

        let status_str: String = row.get("status");
        let status: CommentStatus = status_str.parse()?;

        let deleted_at: Option<String> = row.get("deleted_at");

        Ok(Comment {
            id,
            document_id: row.get("document_id"),
            parent_id,
            author,
            body: row.get("body"),
            selectors,
            status,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            deleted_at,
        })
    }

    /// Create a new comment and return it with generated `id` and timestamps.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidParent`] if `parent_id` is set but the
    /// parent does not exist or belongs to a different document.
    pub async fn create(&self, input: CreateComment) -> Result<Comment, StoreError> {
        if let Some(parent_id) = input.parent_id {
            let row = query("SELECT document_id FROM comments WHERE id = ?")
                .bind(parent_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
            match row {
                Some(row) => {
                    let parent_doc: String = row.get("document_id");
                    if parent_doc != input.document_id {
                        return Err(StoreError::InvalidParent(format!(
                            "parent {parent_id} belongs to a different document"
                        )));
                    }
                }
                None => {
                    return Err(StoreError::InvalidParent(format!(
                        "parent {parent_id} does not exist"
                    )));
                }
            }
        }

        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let author = input.author.unwrap_or_else(Author::local_human);
        let selectors_json = serde_json::to_string(&input.selectors)?;
        let author_json = serde_json::to_string(&author)?;
        let parent_id_str = input.parent_id.map(|id| id.to_string());

        query(
            "INSERT INTO comments (id, document_id, parent_id, body, selectors, author, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&input.document_id)
        .bind(&parent_id_str)
        .bind(&input.body)
        .bind(&selectors_json)
        .bind(&author_json)
        .bind(CommentStatus::Open.as_str())
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(Comment {
            id,
            document_id: input.document_id,
            parent_id: input.parent_id,
            author,
            body: input.body,
            selectors: input.selectors,
            status: CommentStatus::Open,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        })
    }

    /// # Errors
    ///
    /// Returns [`StoreError::NotFound`] if no comment with `id` exists, or if it
    /// exists but has been soft-deleted (rows with `deleted_at IS NOT NULL` are
    /// always hidden).
    pub async fn get(&self, id: Uuid) -> Result<Comment, StoreError> {
        let row = query(
            "SELECT * FROM comments
             WHERE id = ?1
               AND deleted_at IS NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound(id))?;

        Self::row_to_comment(&row)
    }

    /// List comments matching the given filter criteria. Soft-deleted rows
    /// (`deleted_at IS NOT NULL`) are always excluded.
    pub async fn list(&self, filter: CommentFilter) -> Result<Vec<Comment>, StoreError> {
        let parent_id_str = filter.parent_id.map(|id| id.to_string());
        let top_level_only = filter.parent_id.is_none() && filter.top_level_only;

        let rows = query(
            "SELECT * FROM comments
             WHERE (?1 IS NULL OR document_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR parent_id = ?3)
               AND (?4 = 0 OR parent_id IS NULL)
               AND deleted_at IS NULL
             ORDER BY created_at ASC",
        )
        .bind(filter.document_id.as_deref())
        .bind(filter.status.map(CommentStatus::as_str))
        .bind(parent_id_str.as_deref())
        .bind(i64::from(top_level_only))
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_comment).collect()
    }

    /// Update a comment's fields and return the updated comment. Unset fields
    /// in `input` are left unchanged.
    ///
    /// Soft-deleted rows (`deleted_at IS NOT NULL`) are treated as absent: any
    /// update that targets such a row returns [`StoreError::NotFound`] —
    /// *except* the restore case, where the caller sets `status` to
    /// [`CommentStatus::Open`]. Restore clears `deleted_at` and sets
    /// `status='open'` atomically. In this model only replies can be deleted
    /// (top-level comments use Resolve), so restore has no parent-state
    /// preconditions: top-level parents are never deleted.
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotFound`] if no comment with `id` exists, or if it
    ///   exists and is soft-deleted and the caller did not request restore.
    pub async fn update(&self, id: Uuid, input: UpdateComment) -> Result<Comment, StoreError> {
        let now = Utc::now().to_rfc3339();
        let selectors_json = input
            .selectors
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        // Restore branch: only triggered when caller asks for Open AND the row
        // is currently deleted. Try the guarded restore UPDATE first; on empty
        // RETURNING, fall through to the non-restore UPDATE below.
        if matches!(input.status, Some(CommentStatus::Open)) {
            let row = query(
                "UPDATE comments AS c
                    SET status = 'open',
                        deleted_at = NULL,
                        updated_at = ?2
                  WHERE c.id = ?1
                    AND c.deleted_at IS NOT NULL
                RETURNING *",
            )
            .bind(id.to_string())
            .bind(&now)
            .fetch_optional(&self.pool)
            .await?;

            if let Some(row) = row {
                return Self::row_to_comment(&row);
            }
            // Empty RETURNING → row is either missing or already live; the
            // non-restore UPDATE below handles both (NotFound for missing,
            // no-op set-to-open for already-live).
        }

        // Non-restore branch: existing UPDATE plus `deleted_at IS NULL` guard.
        let row = query(
            "UPDATE comments AS c
                SET body = COALESCE(?, body),
                    status = COALESCE(?, status),
                    selectors = COALESCE(?, selectors),
                    updated_at = ?
              WHERE c.id = ?
                AND c.deleted_at IS NULL
          RETURNING *",
        )
        .bind(input.body.as_deref())
        .bind(input.status.map(CommentStatus::as_str))
        .bind(selectors_json.as_deref())
        .bind(&now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound(id))?;

        Self::row_to_comment(&row)
    }

    /// Soft-delete a reply by stamping `deleted_at`.
    ///
    /// Top-level comments (those without a `parent_id`) are never deletable in
    /// this model — use Resolve instead. Attempting to delete a top-level row
    /// returns [`StoreError::NotFound`] (we treat "you tried to delete an
    /// undeletable row" as "no deletable row matched"). `status` is left
    /// untouched — the canonical deleted signal is `deleted_at IS NOT NULL`.
    ///
    /// Returns the resulting comment. Idempotent: deleting an already-deleted
    /// reply returns the existing soft-deleted row without bumping
    /// `updated_at`.
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotFound`] if no comment with `id` exists, or if the
    ///   row exists but is a top-level comment (which can't be deleted).
    pub async fn delete_comment(&self, id: Uuid) -> Result<Comment, StoreError> {
        let now = Utc::now().to_rfc3339();

        // Hot path: guarded single statement. Only operates on rows that
        // currently have a parent (i.e. are replies) and are not already
        // soft-deleted.
        let hot = query(
            "UPDATE comments AS c
                SET deleted_at = ?2,
                    updated_at = ?2
              WHERE c.id = ?1
                AND c.deleted_at IS NULL
                AND c.parent_id IS NOT NULL
            RETURNING *",
        )
        .bind(id.to_string())
        .bind(&now)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = hot {
            return Self::row_to_comment(&row);
        }

        // Cold path: hot-path UPDATE returned no row. Classify why via a
        // best-effort SELECT — under WAL with multiple pool connections a
        // concurrent writer could change the row between UPDATE and SELECT,
        // but the only ambiguity is missing-row vs. concurrent-flip, both of
        // which map to NotFound.
        let diag = query("SELECT * FROM comments WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        let Some(row) = diag else {
            return Err(StoreError::NotFound(id));
        };
        let parent_id_str: Option<String> = row.get("parent_id");
        let already_deleted = row.get::<Option<String>, _>("deleted_at").is_some();

        // Idempotent re-DELETE of a reply — return existing row without
        // bumping updated_at.
        if already_deleted && parent_id_str.is_some() {
            return Self::row_to_comment(&row);
        }

        // Top-level row (parent_id IS NULL) → not deletable, surface as NotFound.
        // Or a concurrent flip on a live reply → also NotFound.
        Err(StoreError::NotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;
    use crate::error::StoreError;
    use crate::model::{
        Author, CommentFilter, CommentStatus, CreateComment, Selector, UpdateComment,
    };
    use std::assert_matches;

    async fn store() -> SqliteCommentStore {
        SqliteCommentStore::open_memory().await.unwrap()
    }

    fn seed(doc: &str, body: &str) -> CreateComment {
        CreateComment {
            document_id: doc.to_owned(),
            parent_id: None,
            author: Some(Author::local_human()),
            body: body.to_owned(),
            selectors: vec![],
        }
    }

    #[tokio::test]
    async fn test_create_and_get_comment() {
        let store = store().await;

        let input = CreateComment {
            selectors: vec![Selector::TextQuoteSelector {
                exact: "some text".to_owned(),
                prefix: "before ".to_owned(),
                suffix: " after".to_owned(),
            }],
            ..seed("docs/guide.md", "This needs clarification.")
        };

        let created = store.create(input).await.unwrap();
        assert_eq!(created.document_id, "docs/guide.md");
        assert_eq!(created.body, "This needs clarification.");
        assert_eq!(created.status, CommentStatus::Open);
        assert_eq!(created.selectors.len(), 1);
        assert!(created.parent_id.is_none());

        let fetched = store.get(created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.document_id, created.document_id);
        assert_eq!(fetched.body, created.body);
        assert_eq!(fetched.selectors, created.selectors);
    }

    #[tokio::test]
    async fn test_list_by_document() {
        let store = store().await;

        store.create(seed("docs/a.md", "Comment A")).await.unwrap();
        store.create(seed("docs/b.md", "Comment B")).await.unwrap();

        let results = store
            .list(CommentFilter {
                document_id: Some("docs/a.md".to_owned()),
                ..CommentFilter::default()
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body, "Comment A");
    }

    #[tokio::test]
    async fn list_top_level_only_drops_replies() {
        let store = store().await;

        let parent = store.create(seed("docs/a.md", "root")).await.unwrap();
        store
            .create(CreateComment {
                parent_id: Some(parent.id),
                ..seed("docs/a.md", "reply")
            })
            .await
            .unwrap();

        let top_level = store
            .list(CommentFilter {
                document_id: Some("docs/a.md".to_owned()),
                top_level_only: true,
                ..CommentFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(top_level.len(), 1);
        assert_eq!(top_level[0].body, "root");

        let thread = store
            .list(CommentFilter {
                parent_id: Some(parent.id),
                ..CommentFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].body, "reply");
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = store().await;

        let created = store
            .create(seed("docs/guide.md", "Fix this."))
            .await
            .unwrap();

        assert_eq!(created.status, CommentStatus::Open);

        let updated = store
            .update(
                created.id,
                UpdateComment {
                    body: None,
                    status: Some(CommentStatus::Resolved),
                    selectors: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.status, CommentStatus::Resolved);
        assert_eq!(updated.body, "Fix this.");
        assert_eq!(updated.author, Author::local_human());
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let store = store().await;
        let result = store.get(Uuid::new_v4()).await;

        assert_matches!(result, Err(StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_author_round_trips_through_create_get_list() {
        let store = store().await;

        let author = Author {
            id: "user:default/mike".to_owned(),
            name: "Mike Yumatov".to_owned(),
            avatar_url: Some("https://example.com/a.png".to_owned()),
        };

        let created = store
            .create(CreateComment {
                author: Some(author.clone()),
                ..seed("docs/guide.md", "Hello")
            })
            .await
            .unwrap();

        assert_eq!(created.author, author);

        let fetched = store.get(created.id).await.unwrap();
        assert_eq!(fetched.author, author);

        let listed = store
            .list(CommentFilter {
                document_id: Some("docs/guide.md".to_owned()),
                ..CommentFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].author, author);

        let created_no_avatar = store.create(seed("docs/guide.md", "Hi")).await.unwrap();
        assert_eq!(created_no_avatar.author.avatar_url, None);
        let re_fetched = store.get(created_no_avatar.id).await.unwrap();
        assert_eq!(re_fetched.author.avatar_url, None);
    }

    #[test]
    fn default_path_joins_project_dir() {
        let p = SqliteCommentStore::default_path(Path::new("/proj/.rw"));
        assert_eq!(p, PathBuf::from("/proj/.rw/comments/sqlite.db"));
    }

    #[test]
    fn store_error_not_found_and_invalid_parent_display_clearly() {
        let id = Uuid::new_v4();
        let a = StoreError::NotFound(id);
        assert!(
            a.to_string().contains("comment not found"),
            "NotFound display = {a}"
        );
        let b = StoreError::InvalidParent("bad parent".to_owned());
        assert!(
            b.to_string().contains("invalid parent"),
            "InvalidParent display = {b}"
        );
    }

    /// Helper: create a parent + reply pair on the same document and return both.
    async fn parent_and_reply(store: &SqliteCommentStore, doc: &str) -> (Comment, Comment) {
        let parent = store.create(seed(doc, "p")).await.unwrap();
        let reply = store
            .create(CreateComment {
                document_id: doc.into(),
                parent_id: Some(parent.id),
                author: None,
                body: "r".into(),
                selectors: vec![],
            })
            .await
            .unwrap();
        (parent, reply)
    }

    #[tokio::test]
    async fn delete_reply_success() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let (_, reply) = parent_and_reply(&store, "a.md").await;
        let deleted = store.delete_comment(reply.id).await.unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(deleted.updated_at >= reply.updated_at);
        // `status` is intentionally left untouched — the signal is now
        // `deleted_at IS NOT NULL`.
        assert_eq!(deleted.status, CommentStatus::Open);
    }

    #[tokio::test]
    async fn delete_missing_is_not_found() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let err = store.delete_comment(Uuid::new_v4()).await.unwrap_err();
        assert_matches!(err, StoreError::NotFound(_));
    }

    #[tokio::test]
    async fn delete_top_level_is_not_found() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let parent = store.create(seed("a.md", "p")).await.unwrap();
        let err = store.delete_comment(parent.id).await.unwrap_err();
        assert_matches!(err, StoreError::NotFound(_));
        // parent row stays open
        let still = store.get(parent.id).await.unwrap();
        assert_eq!(still.status, CommentStatus::Open);
    }

    #[tokio::test]
    async fn delete_already_deleted_reply_is_idempotent_with_unchanged_updated_at() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let (_, reply) = parent_and_reply(&store, "a.md").await;
        let first = store.delete_comment(reply.id).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let second = store.delete_comment(reply.id).await.unwrap();
        assert_eq!(first.updated_at, second.updated_at);
        assert!(second.deleted_at.is_some());
        assert_eq!(first.deleted_at, second.deleted_at);
    }

    #[tokio::test]
    async fn restore_reply_via_update_to_open_succeeds() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let (_, reply) = parent_and_reply(&store, "a.md").await;
        let _ = store.delete_comment(reply.id).await.unwrap();
        let restored = store
            .update(
                reply.id,
                UpdateComment {
                    body: None,
                    status: Some(CommentStatus::Open),
                    selectors: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(restored.status, CommentStatus::Open);
        assert!(restored.deleted_at.is_none());
        assert!(restored.updated_at > reply.updated_at);
    }

    #[tokio::test]
    async fn update_resolve_on_deleted_reply_returns_not_found() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let (_, reply) = parent_and_reply(&store, "a.md").await;
        let _ = store.delete_comment(reply.id).await.unwrap();
        let err = store
            .update(
                reply.id,
                UpdateComment {
                    body: None,
                    status: Some(CommentStatus::Resolved),
                    selectors: None,
                },
            )
            .await
            .unwrap_err();
        assert_matches!(err, StoreError::NotFound(_));
    }

    #[tokio::test]
    async fn update_body_on_deleted_reply_returns_not_found() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let (_, reply) = parent_and_reply(&store, "a.md").await;
        let _ = store.delete_comment(reply.id).await.unwrap();
        let err = store
            .update(
                reply.id,
                UpdateComment {
                    body: Some("new".into()),
                    status: None,
                    selectors: None,
                },
            )
            .await
            .unwrap_err();
        assert_matches!(err, StoreError::NotFound(_));
    }

    #[tokio::test]
    async fn migrate_records_latest_version_on_fresh_db() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let v: i64 = query("SELECT COALESCE(MAX(version), 0) AS v FROM schema_versions")
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .get("v");
        assert_eq!(v, LATEST_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn migrate_is_idempotent_on_reopen() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        // Re-run on the same pool — already fully migrated, so no duplicate
        // rows should appear in schema_versions.
        SqliteCommentStore::migrate(&store.pool).await.unwrap();
        let versions: Vec<i64> = query("SELECT version FROM schema_versions ORDER BY version")
            .fetch_all(&store.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<i64, _>("version"))
            .collect();
        // Check exact sequence [1, 2, ..., LATEST_SCHEMA_VERSION] not just length.
        // Catches gaps like [1, 3] pretending to be valid.
        assert_eq!(
            versions,
            (1..=LATEST_SCHEMA_VERSION).collect::<Vec<_>>(),
            "version sequence must be contiguous from 1 to LATEST_SCHEMA_VERSION"
        );
    }

    #[tokio::test]
    async fn migrate_refuses_db_with_newer_schema() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        // Simulate a future binary having written a higher version.
        query("INSERT INTO schema_versions (version, applied_at) VALUES (?, ?)")
            .bind(LATEST_SCHEMA_VERSION + 1)
            .bind("2999-01-01T00:00:00Z")
            .execute(&store.pool)
            .await
            .unwrap();
        let err = SqliteCommentStore::migrate(&store.pool).await.unwrap_err();
        assert_matches!(
            err,
            StoreError::IncompatibleSchema { db, binary }
            if db == LATEST_SCHEMA_VERSION + 1 && binary == LATEST_SCHEMA_VERSION
        );
    }

    #[tokio::test]
    async fn migrate_upgrades_pre_framework_db() {
        // Simulate a DB written by a pre-framework `rw` binary: the
        // `comments` table and its indexes exist on disk, but
        // `schema_versions` does not. Verify `SqliteCommentStore::open()`
        // lands it at LATEST_SCHEMA_VERSION cleanly with no data loss.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("sqlite.db");

        // Pre-seed the file using the same pool config as production so the
        // test exercises the multi-connection acquire path (max_connections
        // = 4, WAL, synchronous = Normal, busy_timeout = 5s).
        let pre_opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));
        let pre_pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(pre_opts)
            .await
            .unwrap();

        // Mirror exactly what the pre-framework `migrate()` did: the v1 DDL
        // statements, without `schema_versions`.
        for stmt in &[
            "CREATE TABLE IF NOT EXISTS comments (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                parent_id TEXT,
                body TEXT NOT NULL,
                selectors TEXT NOT NULL,
                author TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_comments_document_id ON comments (document_id)",
            "CREATE INDEX IF NOT EXISTS idx_comments_parent_id ON comments (parent_id)",
            "CREATE INDEX IF NOT EXISTS idx_comments_document_status
                ON comments (document_id, status)",
        ] {
            query(*stmt).execute(&pre_pool).await.unwrap();
        }

        // Seed a row so the assertions below actually prove the migration
        // didn't drop+recreate the table. Without seeding, COUNT would be 0
        // either way (data preserved OR table dropped+recreated) and the
        // test would silently pass even on destruction.
        query(
            "INSERT INTO comments
                 (id, document_id, parent_id, body, selectors, author, status, created_at, updated_at)
             VALUES
                 (?, ?, NULL, ?, '[]', '{\"id\":\"local:human\",\"name\":\"You\"}', 'open', ?, ?)",
        )
        .bind("00000000-0000-0000-0000-000000000001")
        .bind("docs/test.md")
        .bind("pre-framework comment body")
        .bind("2025-01-01T00:00:00Z")
        .bind("2025-01-01T00:00:00Z")
        .execute(&pre_pool)
        .await
        .unwrap();

        // Pool::Drop alone only marks-closed synchronously; it does NOT await
        // the sqlite worker thread's actual close. Without the explicit
        // close().await the next SqliteCommentStore::open(&path) can race a
        // still-live background worker.
        pre_pool.close().await;
        drop(pre_pool);

        let store = SqliteCommentStore::open(&db_path).await.unwrap();

        let v: i64 = query("SELECT COALESCE(MAX(version), 0) AS v FROM schema_versions")
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .get("v");
        assert_eq!(v, LATEST_SCHEMA_VERSION);

        // Verify the pre-seeded row survived migration — proves migrate()
        // didn't drop+recreate the table.
        let count: i64 = query("SELECT COUNT(*) AS c FROM comments")
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .get("c");
        assert_eq!(count, 1, "pre-framework comments must survive migration");
        let body: String = query("SELECT body FROM comments LIMIT 1")
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .get("body");
        assert_eq!(body, "pre-framework comment body");

        // Same close-then-drop dance as pre_pool above: SqliteCommentStore
        // holds a SqlitePool internally, and we want its worker thread done
        // with the WAL files before TempDir cleans the directory.
        store.pool.close().await;
        drop(store);
    }

    #[tokio::test]
    async fn migrate_rolls_back_on_failed_statement() {
        // Synthetic migration: one DDL that succeeds, then an invalid SQL
        // statement that errors. With RAII Transaction-on-Drop, the first
        // DDL must NOT persist after the second's failure rolls the txn back.
        const BAD_MIGRATIONS: &[Migration] = &[Migration {
            version: 1,
            stmts: &[
                "CREATE TABLE rollback_probe (id INTEGER PRIMARY KEY)",
                "this is not valid sql",
            ],
        }];

        let store = SqliteCommentStore::open_memory().await.unwrap();
        // Clear the schema_versions row written by the production migrate()
        // that ran during open(), so migrate_with sees current = 0 and
        // actually tries to apply BAD_MIGRATIONS. Leaving the production
        // `comments` table around is fine — BAD_MIGRATIONS doesn't touch
        // it, and the rollback assertions below are about rollback_probe.
        query("DELETE FROM schema_versions")
            .execute(&store.pool)
            .await
            .unwrap();

        let err = SqliteCommentStore::migrate_with(&store.pool, BAD_MIGRATIONS)
            .await
            .unwrap_err();
        assert_matches!(err, StoreError::Sqlx(_));

        // schema_versions still exists (created pre-txn) but contains zero
        // rows: the INSERT inside the txn was rolled back.
        let count: i64 = query("SELECT COUNT(*) AS c FROM schema_versions")
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .get("c");
        assert_eq!(count, 0, "schema_versions must be empty after rollback");

        // rollback_probe must NOT exist in the schema — the CREATE TABLE
        // inside the transaction was rolled back. Query sqlite_master
        // directly rather than catching an error string from a SELECT on
        // the (absent) table, which would rely on SQLite's error wording.
        let table_exists: bool = query(
            "SELECT COUNT(*) > 0 AS e FROM sqlite_master WHERE type='table' AND name='rollback_probe'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .get::<bool, _>("e");
        assert!(
            !table_exists,
            "rollback_probe table must not exist after rollback"
        );
    }
}
