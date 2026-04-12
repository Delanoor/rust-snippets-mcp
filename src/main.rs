use std::sync::{Arc, Mutex};

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use rusqlite::{Connection, params};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::io::{stdin, stdout};

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoArgs {
    message: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SaveSnippetArgs {
    /// short title or description
    title: String,
    /// programming language, e.g. "rust", "bevy", "javascript"
    language: String,
    // actual code or text content
    code: String,
    // Comma-separated tags, e.g. "tilemap,procgen"
    tags: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    tag: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IdArgs {
    // snippet id
    id: i64,
}

#[derive(Clone)]
struct MyServer {
    tool_router: ToolRouter<Self>,
    db: Arc<Mutex<Connection>>,
}

#[tool_router]
impl MyServer {
    fn new(db: Connection) -> anyhow::Result<Self> {
        db.execute(
            "CREATE TABLE IF NOT EXISTS snippets (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            title     TEXT NOT NULL,
            language  TEXT NOT NULL,
            code      TEXT NOT NULL,
            tags      TEXT,
            created   TEXT NOT NULL DEFAULT (datetime('now'))
        )",
            [],
        )?;
        Ok(Self {
            tool_router: Self::tool_router(),
            db: Arc::new(Mutex::new(db)),
        })
    }
    #[tool(description = "Meow")]
    async fn meow(&self) -> String {
        String::from("meow")
    }
    #[tool(description = "Echo (message x 2) back")]
    async fn echo(&self, Parameters(args): Parameters<EchoArgs>) -> String {
        format!("{}{}", args.message, args.message)
    }

    #[tool(description = "Save a code snippet to the local db")]
    async fn save_snippet(
        &self,
        Parameters(args): Parameters<SaveSnippetArgs>,
    ) -> Result<String, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.execute(
            "INSERT INTO snippets (title, language, code, tags) VALUES (?1, ?2, ?3, ?4)",
            params![args.title, args.language, args.code, args.tags],
        )
        .map_err(|e| e.to_string())?;
        let id = db.last_insert_rowid();
        Ok(format!("Saved snippet #{id}: {}", args.title))
    }

    #[tool(description = "Search snippets by text in title or code, optionally filtered by tags")]
    async fn search_snippets(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<String, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let like = format!("%{}%", args.query);
        let tag_like = args.tag.as_ref().map(|t| format!("%{t}%"));

        let mut stmt = db
            .prepare(
                "SELECT id, title, language, tags
             FROM snippets
             WHERE (title LIKE ?1 OR code LIKE ?1)
               AND (?2 IS NULL OR tags LIKE ?2)
             ORDER BY id DESC
             LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![like, tag_like], |row| {
                Ok(format!(
                    "#{} [{}] {} (tags: {})",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            })
            .map_err(|e| e.to_string())?;
        let lines: Vec<String> = rows.filter_map(Result::ok).collect();
        if lines.is_empty() {
            let total: i64 = db
                .query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))
                .unwrap_or(0);
            Ok(format!("No snippets matched. ({total} total in database)"))
        } else {
            Ok(lines.join("\n"))
        }
    }
}

#[tool_handler(
    name = "my server",
    version = "1.0.0",
    instructions = "A simple mcp server"
)]
impl ServerHandler for MyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("myyyy server")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rust-snippets-mcp")
        .join("snippets.db");
    std::fs::create_dir_all(db_path.parent().unwrap())?;
    let db = Connection::open(&db_path)?; // build transport
    let transport = (stdin(), stdout());

    // build a service
    let service = MyServer::new(db)?.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
