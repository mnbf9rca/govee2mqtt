use anyhow::Context;
use chrono::{DateTime, Utc};
use data_encoding::BASE32_NOPAD;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

static CACHE: Lazy<Mutex<Cache>> =
    Lazy::new(|| Mutex::new(open_cache().expect("failed to initialize cache")));

struct Cache {
    connection: Connection,
    revision: u64,
    next_gc: Instant,
}

impl Cache {
    fn new(connection: Connection) -> anyhow::Result<Self> {
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        Ok(Self {
            connection,
            revision: 0,
            next_gc: Instant::now() + Duration::from_secs(900 * 1024),
        })
    }

    fn table(&self, topic: &str) -> anyhow::Result<String> {
        let table = format!("topic_{}", BASE32_NOPAD.encode(topic.as_bytes()));
        self.connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {table} (
                k TEXT PRIMARY KEY NOT NULL,
                v BLOB NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER)),
                expiry INTEGER NOT NULL,
                ttl INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS {table}_by_expiry ON {table} (expiry);"
        ))?;
        Ok(table)
    }

    fn read(&mut self, topic: &str, key: &str) -> anyhow::Result<(u64, Option<Vec<u8>>)> {
        let now = Utc::now().timestamp();
        if Instant::now() >= self.next_gc {
            self.clear_rows(Some(now))?;
            self.next_gc = Instant::now() + Duration::from_secs(900 * 1024);
        }
        let table = self.table(topic)?;
        let value: Option<(Vec<u8>, i64)> = self
            .connection
            .query_row(
                &format!("SELECT v, ttl FROM {table} WHERE k = ?1"),
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((data, ttl)) = value {
            self.connection.execute(
                &format!("UPDATE {table} SET expiry = ?1 WHERE k = ?2"),
                rusqlite::params![now.saturating_add(ttl), key],
            )?;
            Ok((self.revision, Some(data)))
        } else {
            Ok((self.revision, None))
        }
    }

    fn write(
        &self,
        options: CacheGetOptions<'_>,
        data: &[u8],
        revision: u64,
    ) -> anyhow::Result<()> {
        // Purge or invalidation must also discard refreshes already in flight.
        if revision != self.revision {
            return Ok(());
        }
        let table = self.table(options.topic)?;
        let ttl = options.hard_ttl.as_secs().min(i64::MAX as u64) as i64;
        let expiry = Utc::now().timestamp().saturating_add(ttl);
        self.connection.execute(
            &format!("REPLACE INTO {table} (k, v, expiry, ttl) VALUES (?1, ?2, ?3, ?4)"),
            rusqlite::params![options.key, data, expiry, ttl],
        )?;
        Ok(())
    }

    fn invalidate(&mut self, topic: &str, key: &str) -> anyhow::Result<()> {
        let table = self.table(topic)?;
        self.connection
            .execute(&format!("DELETE FROM {table} WHERE k = ?1"), [key])?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn purge(&mut self) -> anyhow::Result<()> {
        self.clear_rows(None)?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn clear_rows(&mut self, expiry_before: Option<i64>) -> anyhow::Result<()> {
        let transaction = self.connection.transaction()?;
        let tables = {
            let mut statement = transaction.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name GLOB 'topic_*'",
            )?;
            let tables = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            tables
        };
        for table in tables {
            let table = table.replace('"', r#""""#);
            transaction.execute(
                &format!(r#"DELETE FROM "{table}" WHERE (?1 IS NULL OR expiry < ?1)"#),
                [expiry_before],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn cache_file_name() -> PathBuf {
    let cache_dir = std::env::var("GOVEE_CACHE_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(dirs_next::cache_dir)
        .expect("failed to resolve cache dir");

    cache_dir.join("govee2mqtt-cache.sqlite")
}

fn open_cache() -> anyhow::Result<Cache> {
    let cache_file = cache_file_name();
    let connection =
        Connection::open(&cache_file).with_context(|| format!("opening {cache_file:?}"))?;
    Cache::new(connection)
}

pub fn purge_cache() -> anyhow::Result<()> {
    CACHE.lock().purge()
}

#[derive(Deserialize, Serialize, Debug)]
struct CacheEntry<T> {
    expires: DateTime<Utc>,
    result: CacheResult<T>,
}

#[derive(Deserialize, Serialize, Debug)]
enum CacheResult<T> {
    Ok(T),
    Err(String),
}

impl<T> CacheResult<T> {
    fn into_result(self) -> anyhow::Result<T> {
        match self {
            Self::Ok(v) => Ok(v),
            Self::Err(err) => anyhow::bail!("{err}"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct CacheGetOptions<'a> {
    pub key: &'a str,
    pub topic: &'a str,
    pub soft_ttl: Duration,
    pub hard_ttl: Duration,
    pub negative_ttl: Duration,
    pub allow_stale: bool,
}

pub enum CacheComputeResult<T> {
    Value(T),
    WithTtl(T, Duration),
}

impl<T> CacheComputeResult<T> {
    #[allow(dead_code)]
    pub fn into_inner(self) -> T {
        match self {
            Self::Value(v) | Self::WithTtl(v, _) => v,
        }
    }
}

pub fn invalidate_key(topic: &str, key: &str) -> anyhow::Result<()> {
    CACHE.lock().invalidate(topic, key)
}

/// Cache an item with a soft TTL; we'll retry the operation
/// if the TTL has expired, but allow stale reads
pub async fn cache_get<T, Fut>(options: CacheGetOptions<'_>, future: Fut) -> anyhow::Result<T>
where
    T: Serialize + DeserializeOwned + std::fmt::Debug + Clone,
    Fut: Future<Output = anyhow::Result<CacheComputeResult<T>>>,
{
    cache_get_with(&CACHE, options, future).await
}

async fn cache_get_with<T, Fut>(
    cache: &Mutex<Cache>,
    options: CacheGetOptions<'_>,
    future: Fut,
) -> anyhow::Result<T>
where
    T: Serialize + DeserializeOwned + std::fmt::Debug + Clone,
    Fut: Future<Output = anyhow::Result<CacheComputeResult<T>>>,
{
    let (revision, current_value) = cache.lock().read(options.topic, options.key)?;
    let now = Utc::now();

    let mut cache_entry: Option<CacheEntry<T>> = None;

    if let Some(current) = &current_value {
        match serde_json::from_slice::<CacheEntry<T>>(current) {
            Ok(entry) => {
                if now < entry.expires {
                    log::trace!("cache hit for {}", options.key);
                    return entry.result.into_result();
                }

                cache_entry.replace(entry);
            }
            Err(err) => {
                log::warn!("Error parsing CacheEntry: {err:#}");
            }
        }
    }

    log::trace!("cache miss for {}", options.key);
    let value: anyhow::Result<CacheComputeResult<T>> = future.await;
    match value {
        Ok(CacheComputeResult::WithTtl(value, ttl)) => {
            let entry = CacheEntry {
                expires: Utc::now() + ttl,
                result: CacheResult::Ok(value.clone()),
            };

            let data = serde_json::to_string_pretty(&entry)?;
            cache.lock().write(options, data.as_bytes(), revision)?;
            Ok(value)
        }
        Ok(CacheComputeResult::Value(value)) => {
            let entry = CacheEntry {
                expires: Utc::now() + options.soft_ttl,
                result: CacheResult::Ok(value.clone()),
            };

            let data = serde_json::to_string_pretty(&entry)?;
            cache.lock().write(options, data.as_bytes(), revision)?;
            Ok(value)
        }
        Err(err) => match cache_entry.take() {
            Some(mut entry) if options.allow_stale => {
                entry.expires = Utc::now() + options.negative_ttl;

                log::warn!("{err:#}, will use prior results");
                if matches!(&entry.result, CacheResult::Err(_)) {
                    entry.result = CacheResult::Err(format!("{err:#}"));
                }

                let data = serde_json::to_string_pretty(&entry)?;
                cache.lock().write(options, data.as_bytes(), revision)?;

                entry.result.into_result()
            }
            _ => {
                let entry = CacheEntry {
                    expires: Utc::now() + options.negative_ttl,
                    result: CacheResult::Err(format!("{err:#}")),
                };

                let data = serde_json::to_string_pretty(&entry)?;
                cache.lock().write(options, data.as_bytes(), revision)?;
                entry.result.into_result()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_cache() -> Mutex<Cache> {
        Mutex::new(Cache::new(Connection::open_in_memory().unwrap()).unwrap())
    }

    fn options() -> CacheGetOptions<'static> {
        CacheGetOptions {
            topic: "test",
            key: "key",
            soft_ttl: Duration::from_secs(300),
            hard_ttl: Duration::from_secs(3600),
            negative_ttl: Duration::from_secs(60),
            allow_stale: true,
        }
    }

    fn stored_entry(cache: &Mutex<Cache>) -> CacheEntry<u32> {
        let (_, data) = cache.lock().read("test", "key").unwrap();
        serde_json::from_slice(&data.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn reads_existing_schema_after_reopening() {
        let path =
            std::env::temp_dir().join(format!("govee-cache-{}.sqlite", uuid::Uuid::new_v4()));
        let entry = CacheEntry {
            expires: Utc::now() + Duration::from_secs(300),
            result: CacheResult::Ok(42u32),
        };
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE topic_ORSXG5A (
                        k TEXT PRIMARY KEY NOT NULL,
                        v BLOB NOT NULL,
                        created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER)),
                        expiry INTEGER NOT NULL,
                        ttl INTEGER NOT NULL
                    );",
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO topic_ORSXG5A (k, v, expiry, ttl) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        "key",
                        serde_json::to_vec_pretty(&entry).unwrap(),
                        Utc::now().timestamp() + 100,
                        3600
                    ],
                )
                .unwrap();
        }
        {
            let cache = Mutex::new(Cache::new(Connection::open(&path).unwrap()).unwrap());
            let value = cache_get_with::<u32, _>(&cache, options(), async {
                anyhow::bail!("a fresh persisted value must avoid a network fetch")
            })
            .await
            .unwrap();
            assert_eq!(value, 42u32);
            let expiry: i64 = cache
                .lock()
                .connection
                .query_row("SELECT expiry FROM topic_ORSXG5A", [], |row| row.get(0))
                .unwrap();
            assert!(expiry >= Utc::now().timestamp() + 3500);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn refreshes_soft_expiry_and_keeps_stale_success_on_error() {
        let cache = memory_cache();
        let entry = CacheEntry {
            expires: Utc::now() - Duration::from_secs(1),
            result: CacheResult::Ok(7u32),
        };
        cache
            .lock()
            .write(options(), &serde_json::to_vec(&entry).unwrap(), 0)
            .unwrap();
        let value = cache_get_with::<u32, _>(&cache, options(), async {
            anyhow::bail!("offline")
        })
        .await
        .unwrap();
        assert_eq!(value, 7);
        assert!(stored_entry(&cache).expires > Utc::now());

        let mut no_stale = options();
        no_stale.allow_stale = false;
        cache
            .lock()
            .write(options(), &serde_json::to_vec(&entry).unwrap(), 0)
            .unwrap();
        let err = cache_get_with::<u32, _>(&cache, no_stale, async {
            anyhow::bail!("new error")
        })
        .await
        .unwrap_err();
        assert_eq!(err.to_string(), "new error");
        let err = cache_get_with::<u32, _>(&cache, no_stale, async {
            Ok(CacheComputeResult::Value(99))
        })
        .await
        .unwrap_err();
        assert_eq!(err.to_string(), "new error");
    }

    #[tokio::test]
    async fn honors_explicit_ttl_and_refreshes_expired_values() {
        let cache = memory_cache();
        let before = Utc::now();
        assert_eq!(
            cache_get_with(&cache, options(), async {
                Ok(CacheComputeResult::WithTtl(3u32, Duration::from_secs(30)))
            })
            .await
            .unwrap(),
            3
        );
        let entry = stored_entry(&cache);
        assert!(entry.expires >= before + Duration::from_secs(30));
        assert!(entry.expires < before + Duration::from_secs(60));
        cache
            .lock()
            .connection
            .execute("UPDATE topic_ORSXG5A SET expiry = 0", [])
            .unwrap();
        // Retention cleanup keeps the previous cache's 900-second x 1024 cadence.
        assert!(cache.lock().read("test", "key").unwrap().1.is_some());
        cache
            .lock()
            .connection
            .execute("UPDATE topic_ORSXG5A SET expiry = 0", [])
            .unwrap();
        cache.lock().next_gc = Instant::now();
        assert_eq!(
            cache_get_with(&cache, options(), async {
                Ok(CacheComputeResult::Value(4u32))
            })
            .await
            .unwrap(),
            4
        );
    }

    #[tokio::test]
    async fn invalidation_and_purge_discard_in_flight_writes() {
        let cache = memory_cache();
        assert_eq!(
            cache_get_with(&cache, options(), async {
                cache.lock().invalidate("test", "key")?;
                Ok(CacheComputeResult::Value(1u32))
            })
            .await
            .unwrap(),
            1
        );
        assert!(cache.lock().read("test", "key").unwrap().1.is_none());
        let mut other = options();
        other.topic = "other";
        cache_get_with(&cache, other, async { Ok(CacheComputeResult::Value(2u32)) })
            .await
            .unwrap();
        cache_get_with(&cache, options(), async {
            cache.lock().purge()?;
            Ok(CacheComputeResult::Value(3u32))
        })
        .await
        .unwrap();
        assert!(cache.lock().read("test", "key").unwrap().1.is_none());
        assert!(cache.lock().read("other", "key").unwrap().1.is_none());
    }

    #[tokio::test]
    async fn corrupt_json_and_large_retention_are_recoverable() {
        let cache = memory_cache();
        let mut options = options();
        options.hard_ttl = Duration::MAX;
        cache.lock().write(options, b"broken", 0).unwrap();
        assert_eq!(
            cache_get_with(&cache, options, async {
                Ok(CacheComputeResult::Value(5u32))
            })
            .await
            .unwrap(),
            5
        );
        let ttl: i64 = cache
            .lock()
            .connection
            .query_row("SELECT ttl FROM topic_ORSXG5A", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ttl, i64::MAX);
    }
}
