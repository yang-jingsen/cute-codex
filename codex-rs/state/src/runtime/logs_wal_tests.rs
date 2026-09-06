use super::*;
use crate::migrations::runtime_logs_migrator;
use crate::runtime::test_support::unique_temp_dir;
use crate::sqlite::LOGS_WAL_JOURNAL_SIZE_LIMIT_BYTES;
use codex_utils_absolute_path::test_support::PathExt;
use pretty_assertions::assert_eq;
use sqlx::Acquire;
use sqlx::SqlitePool;
use std::time::Duration;

fn test_policy() -> LogsWalMaintenancePolicy {
    LogsWalMaintenancePolicy {
        trigger_bytes: 1,
        alert_bytes: u64::MAX,
        busy_timeout: Duration::from_millis(20),
        retry_interval: Duration::ZERO,
    }
}

async fn open_test_logs_database() -> (std::path::PathBuf, crate::SqliteConfig, SqlitePool) {
    let sqlite_home = unique_temp_dir();
    tokio::fs::create_dir_all(&sqlite_home)
        .await
        .expect("create SQLite home");
    let sqlite = crate::SqliteConfig::new_for_testing(sqlite_home.as_path().abs());
    let pool = sqlite
        .open_read_write_pool(&sqlite.logs_db_path())
        .await
        .expect("open logs database");
    sqlx::query("CREATE TABLE wal_test (id INTEGER PRIMARY KEY, payload TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create WAL test table");
    sqlx::query("INSERT INTO wal_test (id, payload) VALUES (0, 'seed')")
        .execute(&pool)
        .await
        .expect("insert seed row");
    sqlx::query_as::<_, (i64, i64, i64)>("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&pool)
        .await
        .expect("start with an empty WAL");
    (sqlite_home, sqlite, pool)
}

#[test]
fn advisory_lock_excludes_concurrent_coordinators_and_defers_retries() {
    let sqlite_home = unique_temp_dir();
    std::fs::create_dir_all(&sqlite_home).expect("create SQLite home");
    let sqlite = crate::SqliteConfig::new_for_testing(sqlite_home.as_path().abs());
    let guard = try_acquire_logs_wal_maintenance_lock(&sqlite)
        .expect("acquire maintenance lock")
        .expect("lock is available");
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let sqlite = sqlite.clone();
            scope.spawn(move || {
                assert!(
                    try_acquire_logs_wal_maintenance_lock(&sqlite)
                        .expect("try concurrent maintenance lock")
                        .is_none()
                );
            });
        }
    });
    drop(guard);
    let mut guard = try_acquire_logs_wal_maintenance_lock(&sqlite)
        .expect("reacquire maintenance lock")
        .expect("lock is available again");
    assert!(
        guard
            .begin_attempt(Duration::from_secs(60))
            .expect("begin attempt")
    );
    drop(guard);
    let mut guard = try_acquire_logs_wal_maintenance_lock(&sqlite)
        .expect("acquire lock for retry")
        .expect("lock is available for retry check");
    assert!(
        !guard
            .begin_attempt(Duration::from_secs(60))
            .expect("defer retry")
    );
    drop(guard);

    std::fs::remove_dir_all(sqlite_home).expect("remove SQLite home");
}

#[tokio::test]
async fn long_reader_defers_truncate_then_quiet_window_shrinks_wal() {
    let (sqlite_home, sqlite, pool) = open_test_logs_database().await;
    let mut reader = pool.acquire().await.expect("acquire reader");
    let mut reader_transaction = reader.begin().await.expect("begin reader transaction");
    let seed = sqlx::query_scalar::<_, String>("SELECT payload FROM wal_test WHERE id = 0")
        .fetch_one(&mut *reader_transaction)
        .await
        .expect("pin reader snapshot");
    assert_eq!(seed, "seed");

    for sequence in 1..=64 {
        sqlx::query("UPDATE wal_test SET payload = ? WHERE id = 0")
            .bind(format!("{sequence}-{}", "x".repeat(8 * 1024)))
            .execute(&pool)
            .await
            .expect("append WAL frame");
    }
    let wal_bytes_before = logs_wal_size(&sqlite).expect("read WAL size");
    assert!(wal_bytes_before > 0);

    let maintenance = {
        let sqlite = sqlite.clone();
        tokio::spawn(async move { maintain_logs_wal(&sqlite, test_policy()).await })
    };
    sqlx::query("UPDATE wal_test SET payload = 'writer-remains-live' WHERE id = 0")
        .execute(&pool)
        .await
        .expect("writer remains live during maintenance");
    let blocked_outcome = tokio::time::timeout(Duration::from_secs(2), maintenance)
        .await
        .expect("reader-blocked maintenance remains bounded")
        .expect("join reader-blocked maintenance")
        .expect("run reader-blocked maintenance");
    let LogsWalMaintenanceOutcome::PassiveIncomplete {
        wal_bytes,
        checkpoint,
    } = blocked_outcome
    else {
        panic!("expected passive checkpoint deferral, got {blocked_outcome:?}");
    };
    assert!(wal_bytes >= wal_bytes_before);
    assert_eq!(checkpoint.busy, 0);
    assert!(checkpoint.log_frames > checkpoint.checkpointed_frames);

    tokio::time::timeout(Duration::from_secs(2), reader_transaction.rollback())
        .await
        .expect("reader rollback remains bounded")
        .expect("release reader snapshot");
    drop(reader);

    let quiet_outcome = tokio::time::timeout(
        Duration::from_secs(2),
        maintain_logs_wal(&sqlite, test_policy()),
    )
    .await
    .expect("quiet-window maintenance remains bounded")
    .expect("run quiet-window maintenance");
    let LogsWalMaintenanceOutcome::Truncated {
        wal_bytes_after,
        passive,
        ..
    } = quiet_outcome
    else {
        panic!("expected quiet-window truncation, got {quiet_outcome:?}");
    };
    assert!(passive.log_frames >= 0);
    assert_eq!(passive.log_frames, passive.checkpointed_frames);
    assert_eq!(wal_bytes_after, 0);

    let payload = sqlx::query_scalar::<_, String>("SELECT payload FROM wal_test WHERE id = 0")
        .fetch_one(&pool)
        .await
        .expect("read post-maintenance row");
    let integrity = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .expect("check database integrity");
    assert_eq!(
        (payload.as_str(), integrity.as_str()),
        ("writer-remains-live", "ok")
    );

    pool.close().await;
    tokio::fs::remove_dir_all(sqlite_home)
        .await
        .expect("remove SQLite home");
}

#[tokio::test]
async fn logs_pool_sets_journal_size_limit_on_every_connection() {
    let sqlite_home = unique_temp_dir();
    tokio::fs::create_dir_all(&sqlite_home)
        .await
        .expect("create SQLite home");
    let sqlite = crate::SqliteConfig::new_for_testing(sqlite_home.as_path().abs());
    let pool = sqlite
        .open_logs_db(&runtime_logs_migrator(), /*telemetry_override*/ None)
        .await
        .expect("open logs database");
    let mut connections = Vec::new();
    let mut limits = Vec::new();
    for _ in 0..5 {
        let mut connection = pool.acquire().await.expect("acquire logs connection");
        limits.push(
            sqlx::query_scalar::<_, i64>("PRAGMA journal_size_limit")
                .fetch_one(&mut *connection)
                .await
                .expect("read journal size limit"),
        );
        connections.push(connection);
    }
    assert_eq!(limits, vec![LOGS_WAL_JOURNAL_SIZE_LIMIT_BYTES; 5]);

    drop(connections);
    pool.close().await;
    tokio::fs::remove_dir_all(sqlite_home)
        .await
        .expect("remove SQLite home");
}
