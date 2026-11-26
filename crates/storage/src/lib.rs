//! Stockage local (SQLite).
//!
//! * Migrations numérotées appliquées au démarrage (`PRAGMA user_version`) : une mise à
//!   jour de l'application **conserve** appareils, historique et préférences.
//! * Une base créée par une version plus récente est refusée au lieu d'être corrompue.
//! * Aucune clé privée ici : seules les clés **publiques** des pairs sont stockées.

use nc_protocol::{DeviceInfo, Platform};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json : {0}")]
    Json(#[from] serde_json::Error),
    #[error("base créée par une version plus récente (schéma {found}, supporté {supported}) — mettez l'application à jour")]
    TooNew { found: u32, supported: u32 },
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Migrations : **ne jamais modifier** une entrée existante, seulement en ajouter.
const MIGRATIONS: &[&str] = &[
    // 1 — schéma initial
    r#"
    CREATE TABLE devices (
        id            TEXT PRIMARY KEY,           -- hex de la clé publique
        public_key    BLOB NOT NULL UNIQUE,
        name          TEXT NOT NULL,
        custom_name   TEXT,
        platform      TEXT NOT NULL,
        manufacturer  TEXT,
        model         TEXT,
        os_version    TEXT,
        app_version   TEXT,
        paired_at     INTEGER NOT NULL,
        last_seen     INTEGER,
        last_addr     TEXT,
        auto_accept   INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE transfers (
        id           TEXT PRIMARY KEY,
        device_id    TEXT NOT NULL,
        outgoing     INTEGER NOT NULL,
        created_at   INTEGER NOT NULL,
        finished_at  INTEGER,
        state        TEXT NOT NULL,
        total_bytes  INTEGER NOT NULL,
        dest_dir     TEXT
    );
    CREATE TABLE transfer_files (
        transfer_id  TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
        idx          INTEGER NOT NULL,
        rel_path     TEXT NOT NULL,
        size         INTEGER NOT NULL,
        source_path  TEXT,
        final_path   TEXT,
        sha256       TEXT,
        state        TEXT NOT NULL,
        PRIMARY KEY (transfer_id, idx)
    );
    CREATE INDEX transfers_created ON transfers(created_at DESC);
    CREATE TABLE notifications (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        device_id  TEXT NOT NULL,
        nkey       TEXT NOT NULL,
        package    TEXT NOT NULL,
        app_name   TEXT NOT NULL,
        title      TEXT,
        body       TEXT,
        posted_at  INTEGER NOT NULL,
        removed    INTEGER NOT NULL DEFAULT 0,
        UNIQUE (device_id, nkey)
    );
    CREATE INDEX notifications_posted ON notifications(posted_at DESC);
    CREATE TABLE notification_apps (
        device_id  TEXT NOT NULL,
        package    TEXT NOT NULL,
        app_name   TEXT NOT NULL,
        enabled    INTEGER NOT NULL,
        PRIMARY KEY (device_id, package)
    );
    "#,
    // 2 — capacités annoncées par l'appareil (JSON)
    r#"ALTER TABLE devices ADD COLUMN capabilities TEXT NOT NULL DEFAULT '[]';"#,
];

pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    #[serde(skip)]
    pub public_key: Vec<u8>,
    pub name: String,
    pub custom_name: Option<String>,
    pub platform: Platform,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
    pub paired_at: i64,
    pub last_seen: Option<i64>,
    pub last_addr: Option<String>,
    pub auto_accept: bool,
    pub capabilities: Vec<nc_protocol::Capability>,
}

impl DeviceRecord {
    pub fn display_name(&self) -> &str {
        self.custom_name.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: Uuid,
    pub device_id: String,
    pub outgoing: bool,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub state: String,
    pub total_bytes: u64,
    pub dest_dir: Option<String>,
    pub files: Vec<TransferFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferFileRecord {
    pub idx: u32,
    pub rel_path: String,
    pub size: u64,
    pub source_path: Option<String>,
    pub final_path: Option<String>,
    pub sha256: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: i64,
    pub device_id: String,
    pub key: String,
    pub package: String,
    pub app_name: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub posted_at: i64,
    pub removed: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn platform_str(p: Platform) -> &'static str {
    match p {
        Platform::Windows => "windows",
        Platform::Android => "android",
    }
}
fn platform_from(s: &str) -> Platform {
    if s == "windows" {
        Platform::Windows
    } else {
        Platform::Android
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    // ---------- appareils ----------

    pub fn upsert_device(
        &self,
        public_key: &[u8],
        info: &DeviceInfo,
        addr: Option<&str>,
    ) -> Result<DeviceRecord> {
        let id = hex::encode(public_key);
        let now = now_ms();
        {
            let c = self.conn.lock().unwrap();
            c.execute(
                "INSERT INTO devices (id, public_key, name, platform, manufacturer, model, os_version, app_version, paired_at, last_seen, last_addr, capabilities)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, platform=excluded.platform, capabilities=excluded.capabilities,
                   manufacturer=excluded.manufacturer, model=excluded.model, os_version=excluded.os_version,
                   app_version=excluded.app_version, last_seen=excluded.last_seen,
                   last_addr=COALESCE(excluded.last_addr, devices.last_addr)",
                params![id, public_key, info.name, platform_str(info.platform), info.manufacturer, info.model, info.os_version, info.app_version, now, addr, serde_json::to_string(&info.capabilities)?],
            )?;
        }
        Ok(self.device(&id)?.expect("inséré"))
    }

    pub fn device(&self, id: &str) -> Result<Option<DeviceRecord>> {
        let c = self.conn.lock().unwrap();
        Ok(
            c.query_row("SELECT * FROM devices WHERE id = ?1", [id], row_device)
                .optional()?,
        )
    }

    pub fn device_by_key(&self, public_key: &[u8]) -> Result<Option<DeviceRecord>> {
        self.device(&hex::encode(public_key))
    }

    pub fn devices(&self) -> Result<Vec<DeviceRecord>> {
        let c = self.conn.lock().unwrap();
        let mut st =
            c.prepare_cached("SELECT * FROM devices ORDER BY COALESCE(last_seen, paired_at) DESC")?;
        let v = st
            .query_map([], row_device)?
            .collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }

    pub fn touch_device(&self, id: &str, addr: Option<&str>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE devices SET last_seen = ?2, last_addr = COALESCE(?3, last_addr) WHERE id = ?1",
            params![id, now_ms(), addr],
        )?;
        Ok(())
    }

    pub fn rename_device(&self, id: &str, custom: Option<&str>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE devices SET custom_name = ?2 WHERE id = ?1",
            params![id, custom],
        )?;
        Ok(())
    }

    pub fn set_auto_accept(&self, id: &str, on: bool) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE devices SET auto_accept = ?2 WHERE id = ?1",
            params![id, on],
        )?;
        Ok(())
    }

    /// Révocation : supprime la clé publique et toutes les données liées à l'appareil.
    pub fn remove_device(&self, id: &str) -> Result<bool> {
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM notifications WHERE device_id = ?1", [id])?;
        tx.execute("DELETE FROM notification_apps WHERE device_id = ?1", [id])?;
        let n = tx.execute("DELETE FROM devices WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    // ---------- préférences ----------

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let c = self.conn.lock().unwrap();
        let v: Option<String> = c
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match v {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        })
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let s = serde_json::to_string(value)?;
        self.conn.lock().unwrap().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, s],
        )?;
        Ok(())
    }

    // ---------- transferts ----------

    pub fn insert_transfer(&self, t: &TransferRecord) -> Result<()> {
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO transfers (id, device_id, outgoing, created_at, finished_at, state, total_bytes, dest_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![t.id.to_string(), t.device_id, t.outgoing, t.created_at, t.finished_at, t.state, t.total_bytes as i64, t.dest_dir],
        )?;
        tx.execute(
            "DELETE FROM transfer_files WHERE transfer_id = ?1",
            [t.id.to_string()],
        )?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO transfer_files (transfer_id, idx, rel_path, size, source_path, final_path, sha256, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for f in &t.files {
                st.execute(params![
                    t.id.to_string(),
                    f.idx,
                    f.rel_path,
                    f.size as i64,
                    f.source_path,
                    f.final_path,
                    f.sha256,
                    f.state
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_transfer_state(&self, id: &Uuid, state: &str, finished: bool) -> Result<()> {
        let fin = finished.then(now_ms);
        self.conn.lock().unwrap().execute(
            "UPDATE transfers SET state = ?2, finished_at = COALESCE(?3, finished_at) WHERE id = ?1",
            params![id.to_string(), state, fin],
        )?;
        Ok(())
    }

    pub fn set_file_result(
        &self,
        id: &Uuid,
        idx: u32,
        state: &str,
        sha256: Option<&str>,
        final_path: Option<&str>,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE transfer_files SET state = ?3, sha256 = COALESCE(?4, sha256), final_path = COALESCE(?5, final_path)
             WHERE transfer_id = ?1 AND idx = ?2",
            params![id.to_string(), idx, state, sha256, final_path],
        )?;
        Ok(())
    }

    pub fn transfer(&self, id: &Uuid) -> Result<Option<TransferRecord>> {
        let c = self.conn.lock().unwrap();
        let t = c
            .query_row(
                "SELECT * FROM transfers WHERE id = ?1",
                [id.to_string()],
                row_transfer,
            )
            .optional()?;
        let Some(mut t) = t else { return Ok(None) };
        t.files = load_files(&c, &t.id)?;
        Ok(Some(t))
    }

    pub fn transfers(&self, limit: u32) -> Result<Vec<TransferRecord>> {
        let c = self.conn.lock().unwrap();
        let mut st =
            c.prepare_cached("SELECT * FROM transfers ORDER BY created_at DESC LIMIT ?1")?;
        let mut v: Vec<TransferRecord> = st
            .query_map([limit], row_transfer)?
            .collect::<std::result::Result<_, _>>()?;
        for t in &mut v {
            t.files = load_files(&c, &t.id)?;
        }
        Ok(v)
    }

    /// Transferts sortants interrompus vers un appareil (pour reprise à la reconnexion).
    pub fn resumable_outgoing(&self, device_id: &str) -> Result<Vec<TransferRecord>> {
        let ids: Vec<String> = {
            let c = self.conn.lock().unwrap();
            let mut st = c.prepare_cached(
                "SELECT id FROM transfers WHERE device_id = ?1 AND outgoing = 1 AND state IN ('interrupted','paused')",
            )?;
            let v = st
                .query_map([device_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            v
        };
        let mut out = Vec::new();
        for id in ids {
            if let Ok(u) = Uuid::parse_str(&id) {
                if let Some(t) = self.transfer(&u)? {
                    out.push(t);
                }
            }
        }
        Ok(out)
    }

    pub fn clear_transfer_history(&self) -> Result<usize> {
        Ok(self.conn.lock().unwrap().execute(
            "DELETE FROM transfers WHERE state IN ('completed','failed','cancelled','rejected')",
            [],
        )?)
    }

    // ---------- notifications ----------

    pub fn insert_notification(
        &self,
        device_id: &str,
        n: &nc_protocol::PhoneNotification,
        keep_content: bool,
    ) -> Result<()> {
        let (title, body) = if keep_content {
            (n.title.as_deref(), n.text.as_deref())
        } else {
            (None, None)
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO notifications (device_id, nkey, package, app_name, title, body, posted_at, removed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
             ON CONFLICT(device_id, nkey) DO UPDATE SET title=excluded.title, body=excluded.body,
               posted_at=excluded.posted_at, removed=0",
            params![device_id, n.key, n.package, n.app_name, title, body, n.posted_at],
        )?;
        Ok(())
    }

    pub fn mark_notification_removed(&self, device_id: &str, key: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE notifications SET removed = 1 WHERE device_id = ?1 AND nkey = ?2",
            params![device_id, key],
        )?;
        Ok(())
    }

    pub fn notifications(&self, limit: u32) -> Result<Vec<NotificationRecord>> {
        let c = self.conn.lock().unwrap();
        let mut st =
            c.prepare_cached("SELECT * FROM notifications ORDER BY posted_at DESC LIMIT ?1")?;
        let v = st
            .query_map([limit], |r| {
                Ok(NotificationRecord {
                    id: r.get("id")?,
                    device_id: r.get("device_id")?,
                    key: r.get("nkey")?,
                    package: r.get("package")?,
                    app_name: r.get("app_name")?,
                    title: r.get("title")?,
                    body: r.get("body")?,
                    posted_at: r.get("posted_at")?,
                    removed: r.get("removed")?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }

    /// Garde au plus `keep` notifications (plus récentes).
    pub fn prune_notifications(&self, keep: u32) -> Result<usize> {
        Ok(self.conn.lock().unwrap().execute(
            "DELETE FROM notifications WHERE id NOT IN (SELECT id FROM notifications ORDER BY posted_at DESC LIMIT ?1)",
            [keep],
        )?)
    }

    pub fn clear_notifications(&self) -> Result<usize> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM notifications", [])?)
    }

    pub fn set_notification_apps(
        &self,
        device_id: &str,
        apps: &[nc_protocol::AppEntry],
    ) -> Result<()> {
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute(
            "DELETE FROM notification_apps WHERE device_id = ?1",
            [device_id],
        )?;
        {
            let mut st = tx.prepare_cached("INSERT INTO notification_apps (device_id, package, app_name, enabled) VALUES (?1, ?2, ?3, ?4)")?;
            for a in apps {
                st.execute(params![device_id, a.package, a.app_name, a.enabled])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn notification_apps(&self, device_id: &str) -> Result<Vec<nc_protocol::AppEntry>> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare_cached("SELECT package, app_name, enabled FROM notification_apps WHERE device_id = ?1 ORDER BY app_name COLLATE NOCASE")?;
        let v = st
            .query_map([device_id], |r| {
                Ok(nc_protocol::AppEntry {
                    package: r.get(0)?,
                    app_name: r.get(1)?,
                    enabled: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }

    pub fn set_notification_app_enabled(
        &self,
        device_id: &str,
        package: &str,
        enabled: bool,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE notification_apps SET enabled = ?3 WHERE device_id = ?1 AND package = ?2",
            params![device_id, package, enabled],
        )?;
        Ok(())
    }

    /// Efface toutes les données locales (appareils, historique, préférences).
    pub fn wipe(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch(
            "DELETE FROM transfer_files; DELETE FROM transfers; DELETE FROM notifications;
             DELETE FROM notification_apps; DELETE FROM devices; DELETE FROM settings;",
        )?;
        Ok(())
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StorageError::TooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as u32)?;
        tx.commit()?;
    }
    Ok(())
}

fn row_device(r: &rusqlite::Row) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: r.get("id")?,
        public_key: r.get("public_key")?,
        name: r.get("name")?,
        custom_name: r.get("custom_name")?,
        platform: platform_from(&r.get::<_, String>("platform")?),
        manufacturer: r.get("manufacturer")?,
        model: r.get("model")?,
        os_version: r.get("os_version")?,
        app_version: r.get("app_version")?,
        paired_at: r.get("paired_at")?,
        last_seen: r.get("last_seen")?,
        last_addr: r.get("last_addr")?,
        auto_accept: r.get("auto_accept")?,
        capabilities: serde_json::from_str(&r.get::<_, String>("capabilities")?)
            .unwrap_or_default(),
    })
}

fn row_transfer(r: &rusqlite::Row) -> rusqlite::Result<TransferRecord> {
    let id: String = r.get("id")?;
    Ok(TransferRecord {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        device_id: r.get("device_id")?,
        outgoing: r.get("outgoing")?,
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        state: r.get("state")?,
        total_bytes: r.get::<_, i64>("total_bytes")? as u64,
        dest_dir: r.get("dest_dir")?,
        files: Vec::new(),
    })
}

fn load_files(c: &Connection, id: &Uuid) -> Result<Vec<TransferFileRecord>> {
    let mut st =
        c.prepare_cached("SELECT * FROM transfer_files WHERE transfer_id = ?1 ORDER BY idx")?;
    let v = st
        .query_map([id.to_string()], |r| {
            Ok(TransferFileRecord {
                idx: r.get("idx")?,
                rel_path: r.get("rel_path")?,
                size: r.get::<_, i64>("size")? as u64,
                source_path: r.get("source_path")?,
                final_path: r.get("final_path")?,
                sha256: r.get("sha256")?,
                state: r.get("state")?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_protocol::{AppEntry, PhoneNotification};

    fn info() -> DeviceInfo {
        DeviceInfo {
            name: "Phone (2)".into(),
            platform: Platform::Android,
            manufacturer: Some("Nothing".into()),
            model: Some("A065".into()),
            os_version: Some("14".into()),
            app_version: "0.1.0".into(),
            capabilities: vec![nc_protocol::Capability::Ring],
        }
    }

    #[test]
    fn data_survives_reopen_like_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nc.db");
        {
            let s = Store::open(&p).unwrap();
            s.upsert_device(&[1; 32], &info(), Some("10.0.0.2:1"))
                .unwrap();
            s.set_setting("theme", &"dark").unwrap();
        }
        let s = Store::open(&p).unwrap();
        assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(s.devices().unwrap().len(), 1);
        assert_eq!(
            s.get_setting::<String>("theme").unwrap().as_deref(),
            Some("dark")
        );
    }

    #[test]
    fn migrates_v1_database_without_losing_devices() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nc.db");
        {
            // Base telle que créée par la version 0.1 (schéma 1 seulement).
            let c = Connection::open(&p).unwrap();
            c.execute_batch(MIGRATIONS[0]).unwrap();
            c.pragma_update(None, "user_version", 1).unwrap();
            c.execute("INSERT INTO devices (id, public_key, name, platform, paired_at) VALUES ('aa', x'aa', 'Ancien', 'android', 1)", []).unwrap();
        }
        let s = Store::open(&p).unwrap();
        assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
        let d = s.device("aa").unwrap().unwrap();
        assert_eq!(d.name, "Ancien");
        assert!(d.capabilities.is_empty());
    }

    #[test]
    fn refuses_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nc.db");
        {
            let c = Connection::open(&p).unwrap();
            c.pragma_update(None, "user_version", SCHEMA_VERSION + 5)
                .unwrap();
        }
        assert!(matches!(Store::open(&p), Err(StorageError::TooNew { .. })));
    }

    #[test]
    fn device_lifecycle_and_revocation() {
        let s = Store::open_in_memory().unwrap();
        let d = s.upsert_device(&[2; 32], &info(), None).unwrap();
        s.rename_device(&d.id, Some("Mon tel")).unwrap();
        s.set_auto_accept(&d.id, true).unwrap();
        let d2 = s.device_by_key(&[2; 32]).unwrap().unwrap();
        assert_eq!(d2.display_name(), "Mon tel");
        assert!(d2.auto_accept);
        s.insert_notification(
            &d.id,
            &PhoneNotification {
                key: "k".into(),
                package: "p".into(),
                app_name: "A".into(),
                title: Some("t".into()),
                text: None,
                posted_at: 1,
                icon_png_b64: None,
                actions: vec![],
                sensitive: false,
            },
            true,
        )
        .unwrap();
        assert!(s.remove_device(&d.id).unwrap());
        assert!(s.device_by_key(&[2; 32]).unwrap().is_none());
        assert!(s.notifications(10).unwrap().is_empty());
    }

    #[test]
    fn transfers_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let id = Uuid::new_v4();
        s.insert_transfer(&TransferRecord {
            id,
            device_id: "d".into(),
            outgoing: true,
            created_at: 5,
            finished_at: None,
            state: "interrupted".into(),
            total_bytes: 10,
            dest_dir: None,
            files: vec![TransferFileRecord {
                idx: 0,
                rel_path: "a.txt".into(),
                size: 10,
                source_path: Some("/a.txt".into()),
                final_path: None,
                sha256: None,
                state: "pending".into(),
            }],
        })
        .unwrap();
        assert_eq!(s.resumable_outgoing("d").unwrap().len(), 1);
        s.set_file_result(&id, 0, "completed", Some("ab"), None)
            .unwrap();
        s.set_transfer_state(&id, "completed", true).unwrap();
        let t = s.transfer(&id).unwrap().unwrap();
        assert_eq!(t.files[0].sha256.as_deref(), Some("ab"));
        assert!(t.finished_at.is_some());
        assert_eq!(s.clear_transfer_history().unwrap(), 1);
    }

    #[test]
    fn sensitive_content_not_stored_when_disabled() {
        let s = Store::open_in_memory().unwrap();
        s.insert_notification(
            "d",
            &PhoneNotification {
                key: "k".into(),
                package: "p".into(),
                app_name: "A".into(),
                title: Some("secret".into()),
                text: Some("x".into()),
                posted_at: 1,
                icon_png_b64: None,
                actions: vec![],
                sensitive: true,
            },
            false,
        )
        .unwrap();
        let n = &s.notifications(1).unwrap()[0];
        assert!(n.title.is_none() && n.body.is_none());
    }

    #[test]
    fn notification_apps_filter() {
        let s = Store::open_in_memory().unwrap();
        s.set_notification_apps(
            "d",
            &[AppEntry {
                package: "com.whatsapp".into(),
                app_name: "WhatsApp".into(),
                enabled: true,
            }],
        )
        .unwrap();
        s.set_notification_app_enabled("d", "com.whatsapp", false)
            .unwrap();
        assert!(!s.notification_apps("d").unwrap()[0].enabled);
    }
}
