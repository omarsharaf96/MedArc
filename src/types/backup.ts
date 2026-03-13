/** TypeScript mirrors of the Rust backup structs in src-tauri/src/commands/backup.rs */

/** Returned by `create_backup` on success. */
export interface BackupResult {
  backupId: string;
  filePath: string;
  /** Encrypted file size in bytes (Rust `u64` — always present). */
  fileSizeBytes: number;
  sha256Digest: string;
  completedAt: string;
}

/** Returned by `restore_backup` on success. */
export interface RestoreResult {
  restoreId: string;
  sourcePath: string;
  completedAt: string;
  integrityVerified: boolean;
}

/** One row from the backup_log table, returned by `list_backups`. */
export interface BackupLogEntry {
  id: string;
  operation: string;
  initiatedBy: string;
  startedAt: string;
  completedAt: string | null;
  status: string;
  filePath: string | null;
  /** Rust `Option<i64>` — nullable. */
  fileSizeBytes: number | null;
  sha256Digest: string | null;
  errorMessage: string | null;
}
