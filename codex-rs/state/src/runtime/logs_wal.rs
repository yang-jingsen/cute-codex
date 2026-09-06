use super::StateRuntime;
use crate::SqliteConfig;
use sqlx::Connection;
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::time::MissedTickBehavior;

const LOGS_WAL_MAINTENANCE_LOCK_FILE: &str = ".logs_2.sqlite-wal-maintenance.lock";
const LOGS_WAL_CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct LogsWalMaintenancePolicy {
    trigger_bytes: u64,
    alert_bytes: u64,
    busy_timeout: Duration,
    retry_interval: Duration,
}

const LOGS_WAL_MAINTENANCE_POLICY: LogsWalMaintenancePolicy = LogsWalMaintenancePolicy {
    trigger_bytes: 64 * 1024 * 1024,
    alert_bytes: 1024 * 1024 * 1024,
    busy_timeout: Duration::from_millis(100),
    retry_interval: Duration::from_secs(5 * 60),
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct WalCheckpointResult {
    busy: i64,
    log_frames: i64,
    checkpointed_frames: i64,
}

impl WalCheckpointResult {
    fn is_complete(self) -> bool {
        self.busy == 0 && self.log_frames >= 0 && self.log_frames == self.checkpointed_frames
    }
}

#[derive(Debug, Eq, PartialEq)]
enum LogsWalMaintenanceOutcome {
    BelowThreshold,
    CoordinatorBusy,
    RetryDeferred,
    PassiveIncomplete {
        wal_bytes: u64,
        checkpoint: WalCheckpointResult,
    },
    TruncateBusy,
    Truncated {
        wal_bytes_after: u64,
        passive: WalCheckpointResult,
    },
}

struct LogsWalMaintenanceGuard {
    file: File,
}

impl LogsWalMaintenanceGuard {
    fn begin_attempt(&mut self, retry_interval: Duration) -> io::Result<bool> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut contents = String::new();
        self.file.read_to_string(&mut contents)?;
        let last_attempt = contents.trim().parse::<u64>().unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if last_attempt != 0
            && last_attempt <= now
            && now.saturating_sub(last_attempt) < retry_interval.as_secs()
        {
            return Ok(false);
        }
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        writeln!(self.file, "{now}")?;
        self.file.flush()?;
        Ok(true)
    }
}

impl StateRuntime {
    pub(crate) fn start_logs_wal_maintenance(self: &Arc<Self>) {
        let logs_pool = Arc::downgrade(&self.logs_pool);
        let sqlite = self.sqlite.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LOGS_WAL_CHECK_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(pool) = logs_pool.upgrade() else {
                    break;
                };
                if pool.is_closed() {
                    break;
                }
                drop(pool);
                if let Err(error) = maintain_logs_wal(&sqlite, LOGS_WAL_MAINTENANCE_POLICY).await {
                    tracing::warn!(
                        path = %sqlite.logs_db_path().display(),
                        %error,
                        "logs WAL maintenance failed; will retry"
                    );
                }
            }
        });
    }
}

async fn maintain_logs_wal(
    sqlite: &SqliteConfig,
    policy: LogsWalMaintenancePolicy,
) -> anyhow::Result<LogsWalMaintenanceOutcome> {
    let wal_bytes = logs_wal_size(sqlite)?;
    if wal_bytes < policy.trigger_bytes {
        return Ok(LogsWalMaintenanceOutcome::BelowThreshold);
    }
    let Some(mut guard) = try_acquire_logs_wal_maintenance_lock(sqlite)? else {
        return Ok(LogsWalMaintenanceOutcome::CoordinatorBusy);
    };
    if !guard.begin_attempt(policy.retry_interval)? {
        return Ok(LogsWalMaintenanceOutcome::RetryDeferred);
    }

    let mut connection = sqlite
        .open_logs_wal_maintenance_connection(policy.busy_timeout)
        .await?;
    let passive = checkpoint(&mut connection, "PRAGMA wal_checkpoint(PASSIVE)").await?;
    if !passive.is_complete() {
        connection.close().await?;
        if wal_bytes >= policy.alert_bytes {
            tracing::warn!(
                path = %sqlite.logs_db_path().display(),
                wal_bytes,
                busy = passive.busy,
                log_frames = passive.log_frames,
                checkpointed_frames = passive.checkpointed_frames,
                "logs WAL remains oversized after a passive checkpoint; will retry"
            );
        }
        return Ok(LogsWalMaintenanceOutcome::PassiveIncomplete {
            wal_bytes,
            checkpoint: passive,
        });
    }

    let truncate = checkpoint(&mut connection, "PRAGMA wal_checkpoint(TRUNCATE)").await?;
    connection.close().await?;
    if truncate.busy != 0 {
        if wal_bytes >= policy.alert_bytes {
            tracing::warn!(
                path = %sqlite.logs_db_path().display(),
                wal_bytes,
                busy = truncate.busy,
                log_frames = truncate.log_frames,
                checkpointed_frames = truncate.checkpointed_frames,
                "logs WAL quiet-window truncate was busy; will retry"
            );
        }
        return Ok(LogsWalMaintenanceOutcome::TruncateBusy);
    }

    let wal_bytes_after = logs_wal_size(sqlite)?;
    tracing::info!(
        path = %sqlite.logs_db_path().display(),
        wal_bytes_before = wal_bytes,
        wal_bytes_after,
        checkpointed_frames = passive.checkpointed_frames,
        "logs WAL maintenance completed"
    );
    Ok(LogsWalMaintenanceOutcome::Truncated {
        wal_bytes_after,
        passive,
    })
}

async fn checkpoint(
    connection: &mut sqlx::SqliteConnection,
    statement: &'static str,
) -> Result<WalCheckpointResult, sqlx::Error> {
    let (busy, log_frames, checkpointed_frames) = sqlx::query_as::<_, (i64, i64, i64)>(statement)
        .fetch_one(connection)
        .await?;
    Ok(WalCheckpointResult {
        busy,
        log_frames,
        checkpointed_frames,
    })
}

fn try_acquire_logs_wal_maintenance_lock(
    sqlite: &SqliteConfig,
) -> io::Result<Option<LogsWalMaintenanceGuard>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(sqlite.home().join(LOGS_WAL_MAINTENANCE_LOCK_FILE))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(LogsWalMaintenanceGuard { file })),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

fn logs_wal_size(sqlite: &SqliteConfig) -> io::Result<u64> {
    let mut wal_path = OsString::from(sqlite.logs_db_path());
    wal_path.push("-wal");
    match std::fs::metadata(wal_path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "logs_wal_tests.rs"]
mod tests;
