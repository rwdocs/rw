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
        query(
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
        )
        .execute(pool)
        .await?;

        query("CREATE INDEX IF NOT EXISTS idx_comments_document_id ON comments (document_id)")
            .execute(pool)
            .await?;

        query("CREATE INDEX IF NOT EXISTS idx_comments_parent_id ON comments (parent_id)")
            .execute(pool)
            .await?;

        query(
            "CREATE INDEX IF NOT EXISTS idx_comments_document_status
             ON comments (document_id, status)",
        )
        .execute(pool)
        .await?;

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
        })
    }

    /// # Errors
    ///
    /// Returns [`StoreError::NotFound`] if no comment with `id` exists.
    pub async fn get(&self, id: Uuid) -> Result<Comment, StoreError> {
        let row = query("SELECT * FROM comments WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::NotFound(id))?;

        Self::row_to_comment(&row)
    }

    /// List comments matching the given filter criteria.
    pub async fn list(&self, filter: CommentFilter) -> Result<Vec<Comment>, StoreError> {
        let parent_id_str = filter.parent_id.map(|id| id.to_string());
        let top_level_only = filter.parent_id.is_none() && filter.top_level_only;

        let rows = query(
            "SELECT * FROM comments
             WHERE (?1 IS NULL OR document_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR parent_id = ?3)
               AND (?4 = 0 OR parent_id IS NULL)
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
    /// # Errors
    ///
    /// Returns [`StoreError::NotFound`] if no comment with `id` exists.
    pub async fn update(&self, id: Uuid, input: UpdateComment) -> Result<Comment, StoreError> {
        let now = Utc::now().to_rfc3339();
        let selectors_json = input
            .selectors
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let row = query(
            "UPDATE comments
                SET body = COALESCE(?, body),
                    status = COALESCE(?, status),
                    selectors = COALESCE(?, selectors),
                    updated_at = ?
              WHERE id = ?
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

        assert!(matches!(result, Err(StoreError::NotFound(_))));
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
}
