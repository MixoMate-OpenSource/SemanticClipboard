use crate::db::Database;
use crate::ml::MLEngine;
use arboard::Clipboard;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;

use std::sync::atomic::{AtomicBool, Ordering};

pub struct ClipboardMonitor {
    db: Arc<Mutex<Database>>,
    ml: Arc<Mutex<Option<MLEngine>>>,
    needs_refresh: Arc<AtomicBool>,
}

impl ClipboardMonitor {
    pub fn new(db: Arc<Mutex<Database>>, ml: Arc<Mutex<Option<MLEngine>>>, needs_refresh: Arc<AtomicBool>) -> Self {
        Self { db, ml, needs_refresh }
    }

    pub fn spawn(self) {
        // We spawn a standard blocking thread because arboard blocks and is synchronous
        std::thread::spawn(move || {
            let mut clipboard = match Clipboard::new() {
                Ok(cb) => cb,
                Err(e) => {
                    eprintln!("Failed to initialize clipboard: {}", e);
                    return;
                }
            };

            let mut last_text = String::new();

            loop {
                // Poll every 1.5 seconds as per requirements
                std::thread::sleep(Duration::from_millis(1500));

                if let Ok(current_text) = clipboard.get_text() {
                    let current_text = current_text.trim();
                    if !current_text.is_empty() && current_text != last_text {
                        last_text = current_text.to_string();
                        
                        // We have a new clipboard entry
                        // Create embedding
                        let embedding_result = {
                            let mut ml_lock = self.ml.lock().unwrap();
                            if let Some(ref mut ml) = *ml_lock {
                                ml.embed(current_text)
                            } else {
                                continue;
                            }
                        };

                        match embedding_result {
                            Ok(embedding) => {
                                // Check settings
                                let cache_only_pinned = {
                                    let lock = self.db.lock().unwrap();
                                    lock.get_setting("cache_only_pinned").unwrap_or_default() == Some("true".to_string())
                                };

                                if !cache_only_pinned {
                                    // Save to DB
                                    let db_lock = self.db.lock().unwrap();
                                    let _ = db_lock.insert_entry(current_text, &embedding);
                                    
                                    let limit_str = db_lock.get_setting("history_limit").unwrap_or_default().unwrap_or_else(|| "10000".to_string());
                                    let limit = limit_str.parse::<usize>().unwrap_or(10000);
                                    let _ = db_lock.cleanup_old_entries(limit);
                                    
                                    // Signal UI to refresh
                                    self.needs_refresh.store(true, Ordering::SeqCst);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create embedding for clipboard text: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }
}
