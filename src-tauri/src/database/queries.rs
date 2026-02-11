use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub platform: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub chrome_profile_dir: String,
    pub is_logged_in: bool,
    pub last_checked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishTask {
    pub id: i64,
    pub video_path: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<String>,
    pub cover_path: Option<String>,
    pub is_original: bool,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlatform {
    pub id: i64,
    pub task_id: i64,
    pub account_id: i64,
    pub custom_title: Option<String>,
    pub custom_description: Option<String>,
    pub custom_tags: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub published_at: Option<String>,
}

// ========== Account Queries ==========

pub fn insert_account(
    conn: &Connection,
    platform: &str,
    display_name: &str,
    chrome_profile_dir: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (platform, display_name, chrome_profile_dir) VALUES (?1, ?2, ?3)",
        params![platform, display_name, chrome_profile_dir],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_all_accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, platform, display_name, avatar_url, chrome_profile_dir, is_logged_in, last_checked_at, created_at FROM accounts ORDER BY created_at DESC"
    )?;
    let accounts = stmt
        .query_map([], |row| {
            Ok(Account {
                id: row.get(0)?,
                platform: row.get(1)?,
                display_name: row.get(2)?,
                avatar_url: row.get(3)?,
                chrome_profile_dir: row.get(4)?,
                is_logged_in: row.get(5)?,
                last_checked_at: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(accounts)
}

pub fn update_account_login_status(conn: &Connection, id: i64, is_logged_in: bool) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET is_logged_in = ?1, last_checked_at = datetime('now') WHERE id = ?2",
        params![is_logged_in, id],
    )?;
    Ok(())
}

pub fn update_account_display_name(conn: &Connection, id: i64, display_name: &str) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET display_name = ?1 WHERE id = ?2",
        params![display_name, id],
    )?;
    Ok(())
}

pub fn delete_account(conn: &Connection, id: i64) -> Result<String> {
    // Get profile dir before deleting
    let profile_dir: String = conn.query_row(
        "SELECT chrome_profile_dir FROM accounts WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
    Ok(profile_dir)
}

// ========== Publish Task Queries ==========

pub fn insert_publish_task(
    conn: &Connection,
    video_path: &str,
    title: &str,
    description: Option<&str>,
    tags: Option<&str>,
    is_original: bool,
    scheduled_at: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO publish_tasks (video_path, title, description, tags, is_original, scheduled_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![video_path, title, description, tags, is_original, scheduled_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_task_platform(conn: &Connection, task_id: i64, account_id: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO publish_task_platforms (task_id, account_id) VALUES (?1, ?2)",
        params![task_id, account_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_task_platform_status(
    conn: &Connection,
    id: i64,
    status: &str,
    error_message: Option<&str>,
) -> Result<()> {
    if status == "published" {
        conn.execute(
            "UPDATE publish_task_platforms SET status = ?1, published_at = datetime('now') WHERE id = ?2",
            params![status, id],
        )?;
    } else {
        conn.execute(
            "UPDATE publish_task_platforms SET status = ?1, error_message = ?2 WHERE id = ?3",
            params![status, error_message, id],
        )?;
    }
    Ok(())
}

pub fn update_task_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE publish_tasks SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn get_all_tasks(conn: &Connection) -> Result<Vec<PublishTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, video_path, title, description, tags, cover_path, is_original, status, scheduled_at, created_at FROM publish_tasks ORDER BY created_at DESC"
    )?;
    let tasks = stmt
        .query_map([], |row| {
            Ok(PublishTask {
                id: row.get(0)?,
                video_path: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                tags: row.get(4)?,
                cover_path: row.get(5)?,
                is_original: row.get(6)?,
                status: row.get(7)?,
                scheduled_at: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(tasks)
}
