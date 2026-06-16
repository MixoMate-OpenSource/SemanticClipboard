use directories::ProjectDirs;
use rusqlite::{params, Connection, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub id: i64,
    pub timestamp: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub is_pinned: bool,
    pub obscured_label: Option<String>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path();
        
        // Ensure parent directories exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_default();
        }

        let conn = Connection::open(&db_path)?;
        
        // Enable Write-Ahead Logging for better concurrent performance and safety
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        let mut db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn get_db_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "SemanticClipboard", "SemanticClipboard") {
            proj_dirs.data_local_dir().join("history.db")
        } else {
            // Fallback to current directory if we can't find the user's data directory
            PathBuf::from("history.db")
        }
    }

    fn init_schema(&mut self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                content TEXT NOT NULL UNIQUE,
                embedding BLOB NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Add obscured_label column if it doesn't exist (ignore error if it does)
        let _ = self.conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN obscured_label TEXT",
            [],
        );

        Ok(())
    }

    pub fn insert_entry(&self, content: &str, embedding: &[f32]) -> Result<i64> {
        // Convert f32 array to raw bytes
        let embedding_bytes: &[u8] = bytemuck::cast_slice(embedding);
        
        // Use standard RFC3339 for timestamp
        let timestamp = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT OR IGNORE INTO clipboard_history (timestamp, content, embedding, is_pinned)
             VALUES (?1, ?2, ?3, 0)",
            params![timestamp, content, embedding_bytes],
        )?;

        // Return the last inserted row id
        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    pub fn get_all_entries(&self) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp, content, embedding, is_pinned, obscured_label FROM clipboard_history ORDER BY id DESC")?;
        
        let entries = stmt.query_map([], |row| {
            let embedding_bytes: Vec<u8> = row.get(3)?;
            let embedding: Vec<f32> = bytemuck::cast_slice(&embedding_bytes).to_vec();
            
            Ok(ClipboardEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                embedding,
                is_pinned: row.get::<_, i32>(4)? != 0,
                obscured_label: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        
        Ok(result)
    }

    pub fn toggle_pin(&self, id: i64, pin: bool) -> Result<()> {
        let pin_val = if pin { 1 } else { 0 };
        self.conn.execute(
            "UPDATE clipboard_history SET is_pinned = ?1 WHERE id = ?2",
            params![pin_val, id],
        )?;
        Ok(())
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM clipboard_history WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn set_obscure_label(&self, id: i64, label: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE clipboard_history SET obscured_label = ?1 WHERE id = ?2",
            params![label, id],
        )?;
        Ok(())
    }

    pub fn cleanup_old_entries(&self, max_unpinned: usize) -> Result<()> {
        // Count unpinned entries
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE is_pinned = 0",
            [],
            |row| row.get(0),
        )?;

        if count > max_unpinned {
            let to_delete = count - max_unpinned;
            self.conn.execute(
                "DELETE FROM clipboard_history 
                 WHERE id IN (
                     SELECT id FROM clipboard_history 
                     WHERE is_pinned = 0 
                     ORDER BY id ASC 
                     LIMIT ?1
                 )",
                params![to_delete],
            )?;
        }
        
        Ok(())
    }

    pub fn clear_unpinned_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clipboard_history WHERE is_pinned = 0", [])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_db_size_bytes(&self) -> usize {
        let path = dirs::data_local_dir().unwrap().join("SemanticClipboard").join("history.db");
        std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0)
    }
}
