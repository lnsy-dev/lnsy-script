use rquickjs::{class::Trace, Ctx, JsLifetime};
use std::str::FromStr;

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct Database {
    #[qjs(skip_trace)]
    pool: sqlx::SqlitePool,
    #[qjs(skip_trace)]
    rt: tokio::runtime::Runtime,
}

#[rquickjs::methods]
impl Database {
    #[qjs(constructor)]
    pub fn new(path: rquickjs::prelude::Opt<String>) -> rquickjs::Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "Database", e.to_string()))?;

        let url = match path.0 {
            Some(p) => format!("sqlite:{}", p),
            None => "sqlite::memory:".to_string(),
        };

        let pool = rt.block_on(async {
            sqlx::SqlitePool::connect_with(
                sqlx::sqlite::SqliteConnectOptions::from_str(&url)
                    .map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?
                    .create_if_missing(true),
            ).await
        }).map_err(|e| rquickjs::Error::new_from_js_message("error", "Database", e.to_string()))?;

        rt.block_on(async {
            // Performance PRAGMAs
            sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
            sqlx::query("PRAGMA cache_size=-65536").execute(&pool).await?;
            sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;
            sqlx::query("PRAGMA temp_store=MEMORY").execute(&pool).await?;

            // Main records table
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS records (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    data TEXT NOT NULL
                )"
            ).execute(&pool).await?;

            // FTS5 virtual table for full-text search
            sqlx::query(
                "CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
                    data, content=records, content_rowid=id
                )"
            ).execute(&pool).await?;
            sqlx::query(
                "CREATE TRIGGER IF NOT EXISTS records_ai AFTER INSERT ON records BEGIN
                    INSERT INTO records_fts(rowid, data) VALUES (new.id, new.data);
                END"
            ).execute(&pool).await?;

            // Index table for fast key-value lookups (replaces json_extract full scan)
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS records_index (
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    record_id INTEGER NOT NULL
                )"
            ).execute(&pool).await?;
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS records_index_kv ON records_index(key, value)"
            ).execute(&pool).await
        }).map_err(|e| rquickjs::Error::new_from_js_message("error", "Database", e.to_string()))?;

        Ok(Database { pool, rt })
    }

    #[qjs(rename = "__addItemSync")]
    pub fn add_item_sync(&self, json_str: String) -> rquickjs::Result<i64> {
        let data: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addItem", e.to_string()))?;
        self.rt.block_on(async {
            let mut tx = self.pool.begin().await?;
            let row = sqlx::query("INSERT INTO records (data) VALUES (?)")
                .bind(&json_str)
                .execute(&mut *tx)
                .await?;
            let id = row.last_insert_rowid();
            // Populate the index for each key-value pair
            if let Some(obj) = data.as_object() {
                for (key, val) in obj {
                    let val_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    sqlx::query(
                        "INSERT INTO records_index (key, value, record_id) VALUES (?, ?, ?)"
                    )
                    .bind(key)
                    .bind(&val_str)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            tx.commit().await?;
            Ok::<i64, sqlx::Error>(id)
        }).map_err(|e| rquickjs::Error::new_from_js_message("error", "addItem", e.to_string()))
    }

    #[qjs(rename = "__findSync")]
    pub fn find_sync(&self, key: String, value: String) -> rquickjs::Result<String> {
        let results = self.rt.block_on(async {
            sqlx::query_scalar::<_, String>(
                "SELECT r.data FROM records r
                 JOIN records_index ri ON r.id = ri.record_id
                 WHERE ri.key = ? AND ri.value = ?"
            )
            .bind(&key)
            .bind(&value)
            .fetch_all(&self.pool)
            .await
        }).map_err(|e| rquickjs::Error::new_from_js_message("error", "find", e.to_string()))?;

        let parsed: Vec<serde_json::Value> = results
            .into_iter()
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();
        serde_json::to_string(&parsed)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "find", e.to_string()))
    }

    #[qjs(rename = "__searchSync")]
    pub fn search_sync(&self, query: String) -> rquickjs::Result<String> {
        let results = self.rt.block_on(async {
            sqlx::query_scalar::<_, String>(
                "SELECT records.data FROM records
                 JOIN records_fts ON records.id = records_fts.rowid
                 WHERE records_fts MATCH ?
                 ORDER BY rank"
            )
            .bind(&query)
            .fetch_all(&self.pool)
            .await
        }).map_err(|e| rquickjs::Error::new_from_js_message("error", "search", e.to_string()))?;

        let parsed: Vec<serde_json::Value> = results
            .into_iter()
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();
        serde_json::to_string(&parsed)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "search", e.to_string()))
    }
}

pub fn setup_database(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<Database>::define(&ctx.globals())?;
    ctx.eval::<(), _>(r#"
Database.prototype.addItem = function(obj) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__addItemSync(JSON.stringify(obj))); }
        catch(e) { reject(e); }
    });
};
Database.prototype.find = function(key, value) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__findSync(key, String(value)))); }
        catch(e) { reject(e); }
    });
};
Database.prototype.search = function(query) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__searchSync(query))); }
        catch(e) { reject(e); }
    });
};
"#)?;
    Ok(())
}
