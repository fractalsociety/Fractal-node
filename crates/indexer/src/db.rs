//! SQLite persistence for indexed blocks and native events.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection};
use serde_json::Value;

pub struct IndexerDb {
    conn: Mutex<Connection>,
}

#[derive(Clone, Debug)]
pub struct BlockRow {
    pub number: u64,
    pub hash: String,
    pub timestamp_ms: u64,
    pub tx_count: u32,
}

#[derive(Clone, Debug)]
pub struct TxRow {
    pub hash: String,
    pub block_number: u64,
    pub tx_index: u32,
    pub signer: String,
    pub vm_kind: String,
    pub call_kind: Option<String>,
    pub payload_json: String,
    pub receipt_status: Option<u32>,
    pub gas_used: Option<u64>,
    pub transfer_to: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SearchResult {
    Block(BlockRow),
    Transaction(TxRow),
    Address {
        address: String,
        transactions: Vec<TxRow>,
    },
}

#[derive(Clone, Debug)]
pub struct ReputationRow {
    pub row_key: String,
    pub last_block: u64,
    pub score_milli: String,
    pub ledger_commitment_hex: String,
    pub ledger_borsh_hex: Option<String>,
    pub client_requesters_hex: Vec<String>,
    pub kind: String,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug)]
pub struct IndexerStatus {
    pub last_indexed_block: u64,
    pub tx_count: u64,
    pub wallet_event_count: u64,
    pub reputation_row_count: u64,
}

impl IndexerDb {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.conn.lock().map_err(|e| e.to_string())
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS blocks (
              number INTEGER PRIMARY KEY,
              hash TEXT NOT NULL,
              timestamp_ms INTEGER NOT NULL,
              tx_count INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS transactions (
              hash TEXT PRIMARY KEY,
              block_number INTEGER NOT NULL,
              tx_index INTEGER NOT NULL,
              signer TEXT NOT NULL,
              vm_kind TEXT NOT NULL,
              call_kind TEXT,
              payload_json TEXT NOT NULL,
              is_wallet INTEGER NOT NULL DEFAULT 0,
              receipt_status INTEGER,
              gas_used INTEGER,
              transfer_to TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tx_block ON transactions(block_number);
            CREATE INDEX IF NOT EXISTS idx_tx_call_kind ON transactions(call_kind);
            CREATE INDEX IF NOT EXISTS idx_tx_wallet ON transactions(is_wallet);
            CREATE INDEX IF NOT EXISTS idx_blocks_hash ON blocks(hash);
            CREATE INDEX IF NOT EXISTS idx_tx_signer ON transactions(signer);
            CREATE INDEX IF NOT EXISTS idx_tx_transfer_to ON transactions(transfer_to);
            CREATE TABLE IF NOT EXISTS reputation_rows (
              row_key TEXT PRIMARY KEY,
              last_block INTEGER NOT NULL,
              score_milli TEXT NOT NULL,
              ledger_commitment_hex TEXT NOT NULL,
              ledger_borsh_hex TEXT,
              client_requesters_json TEXT NOT NULL,
              kind TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|e| e.to_string())?;
        Self::migrate_v2_conn(&conn)?;
        Ok(())
    }

    fn migrate_v2_conn(conn: &Connection) -> Result<(), String> {
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        if version >= 2 {
            return Ok(());
        }
        let _ = conn.execute_batch(
            r#"
            ALTER TABLE transactions ADD COLUMN receipt_status INTEGER;
            ALTER TABLE transactions ADD COLUMN gas_used INTEGER;
            ALTER TABLE transactions ADD COLUMN transfer_to TEXT;
            "#,
        );
        conn.execute("PRAGMA user_version = 2", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn map_tx_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TxRow> {
        Ok(TxRow {
            hash: row.get(0)?,
            block_number: row.get(1)?,
            tx_index: row.get(2)?,
            signer: row.get(3)?,
            vm_kind: row.get(4)?,
            call_kind: row.get(5)?,
            payload_json: row.get(6)?,
            receipt_status: row.get(7)?,
            gas_used: row.get(8)?,
            transfer_to: row.get(9)?,
        })
    }

    const TX_SELECT: &'static str =
        "SELECT hash, block_number, tx_index, signer, vm_kind, call_kind, payload_json,
                receipt_status, gas_used, transfer_to FROM transactions";

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT value FROM meta WHERE key = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![key]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            return Ok(Some(row.get(0).map_err(|e| e.to_string())?));
        }
        Ok(None)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn last_indexed_block(&self) -> Result<u64, String> {
        Ok(self
            .get_meta("last_indexed_block")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    pub fn set_last_indexed_block(&self, n: u64) -> Result<(), String> {
        self.set_meta("last_indexed_block", &n.to_string())
    }

    pub fn insert_block(&self, block: &BlockRow) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO blocks(number, hash, timestamp_ms, tx_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                block.number,
                block.hash,
                block.timestamp_ms,
                block.tx_count,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_tx(&self, tx: &TxRow, is_wallet: bool) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO transactions
             (hash, block_number, tx_index, signer, vm_kind, call_kind, payload_json, is_wallet,
              receipt_status, gas_used, transfer_to)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                tx.hash,
                tx.block_number,
                tx.tx_index,
                tx.signer,
                tx.vm_kind,
                tx.call_kind,
                tx.payload_json,
                if is_wallet { 1i32 } else { 0i32 },
                tx.receipt_status,
                tx.gas_used,
                tx.transfer_to,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn status(&self) -> Result<IndexerStatus, String> {
        let last_indexed_block = self.last_indexed_block()?;
        let conn = self.lock()?;
        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let wallet_event_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE is_wallet = 1",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let reputation_row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM reputation_rows", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(IndexerStatus {
            last_indexed_block,
            tx_count: tx_count.max(0) as u64,
            wallet_event_count: wallet_event_count.max(0) as u64,
            reputation_row_count: reputation_row_count.max(0) as u64,
        })
    }

    pub fn block(&self, number: u64) -> Result<Option<BlockRow>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT number, hash, timestamp_ms, tx_count FROM blocks WHERE number = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![number])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            return Ok(Some(BlockRow {
                number: row.get(0).map_err(|e| e.to_string())?,
                hash: row.get(1).map_err(|e| e.to_string())?,
                timestamp_ms: row.get(2).map_err(|e| e.to_string())?,
                tx_count: row.get(3).map_err(|e| e.to_string())?,
            }));
        }
        Ok(None)
    }

    pub fn blocks(&self, limit: i64, offset: i64) -> Result<Vec<BlockRow>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT number, hash, timestamp_ms, tx_count FROM blocks
                 ORDER BY number DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit, offset], |row| {
                Ok(BlockRow {
                    number: row.get(0)?,
                    hash: row.get(1)?,
                    timestamp_ms: row.get(2)?,
                    tx_count: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn block_by_hash(&self, hash: &str) -> Result<Option<BlockRow>, String> {
        let conn = self.lock()?;
        let needle = normalize_hash_lookup(hash);
        let mut stmt = conn
            .prepare(
                "SELECT number, hash, timestamp_ms, tx_count FROM blocks
                 WHERE lower(hash) = lower(?1) LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![needle])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            return Ok(Some(BlockRow {
                number: row.get(0).map_err(|e| e.to_string())?,
                hash: row.get(1).map_err(|e| e.to_string())?,
                timestamp_ms: row.get(2).map_err(|e| e.to_string())?,
                tx_count: row.get(3).map_err(|e| e.to_string())?,
            }));
        }
        Ok(None)
    }

    pub fn transaction(&self, hash: &str) -> Result<Option<TxRow>, String> {
        let conn = self.lock()?;
        let needle = normalize_hash_lookup(hash);
        let sql = format!("{} WHERE lower(hash) = lower(?1)", Self::TX_SELECT);
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![needle]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            return Ok(Some(Self::map_tx_row(&row).map_err(|e| e.to_string())?));
        }
        Ok(None)
    }

    pub fn transactions_for_block(
        &self,
        block_number: u64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TxRow>, String> {
        let conn = self.lock()?;
        let sql = format!(
            "{} WHERE block_number = ?3 ORDER BY tx_index ASC LIMIT ?1 OFFSET ?2",
            Self::TX_SELECT
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit, offset, block_number], Self::map_tx_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn transactions_for_address(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TxRow>, String> {
        let addr = normalize_address_lookup(address)?;
        let conn = self.lock()?;
        let sql = format!(
            "{} WHERE lower(signer) = lower(?3) OR lower(transfer_to) = lower(?3)
             ORDER BY block_number DESC, tx_index DESC LIMIT ?1 OFFSET ?2",
            Self::TX_SELECT
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit, offset, addr], Self::map_tx_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn transactions(
        &self,
        limit: i64,
        offset: i64,
        call_kind: Option<&str>,
        wallet_only: bool,
    ) -> Result<Vec<TxRow>, String> {
        let conn = self.lock()?;
        let base = Self::TX_SELECT;
        let sql = if wallet_only {
            if call_kind.is_some() {
                format!(
                    "{base} WHERE is_wallet = 1 AND call_kind = ?3
                     ORDER BY block_number DESC, tx_index DESC LIMIT ?1 OFFSET ?2"
                )
            } else {
                format!(
                    "{base} WHERE is_wallet = 1
                     ORDER BY block_number DESC, tx_index DESC LIMIT ?1 OFFSET ?2"
                )
            }
        } else if call_kind.is_some() {
            format!(
                "{base} WHERE call_kind = ?3
                 ORDER BY block_number DESC, tx_index DESC LIMIT ?1 OFFSET ?2"
            )
        } else {
            format!("{base} ORDER BY block_number DESC, tx_index DESC LIMIT ?1 OFFSET ?2")
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = if let Some(ck) = call_kind {
            stmt.query_map(params![limit, offset, ck], Self::map_tx_row)
        } else {
            stmt.query_map(params![limit, offset], Self::map_tx_row)
        }
        .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn search(&self, raw: &str) -> Result<Option<SearchResult>, String> {
        let s = raw.trim();
        if is_tx_hash_query(s) {
            if let Some(tx) = self.transaction(s)? {
                return Ok(Some(SearchResult::Transaction(tx)));
            }
            if let Some(block) = self.block_by_hash(s)? {
                return Ok(Some(SearchResult::Block(block)));
            }
        }
        if let Some(n) = parse_block_number(s) {
            if let Some(block) = self.block(n)? {
                return Ok(Some(SearchResult::Block(block)));
            }
        }
        if let Ok(addr) = normalize_address_lookup(s) {
            let txs = self.transactions_for_address(&addr, 25, 0)?;
            return Ok(Some(SearchResult::Address {
                address: addr,
                transactions: txs,
            }));
        }
        Ok(None)
    }

    pub fn upsert_reputation_row(&self, row: &ReputationRow) -> Result<(), String> {
        let conn = self.lock()?;
        let clients_json =
            serde_json::to_string(&row.client_requesters_hex).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO reputation_rows
             (row_key, last_block, score_milli, ledger_commitment_hex, ledger_borsh_hex, client_requesters_json, kind, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(row_key) DO UPDATE SET
               last_block = excluded.last_block,
               score_milli = excluded.score_milli,
               ledger_commitment_hex = excluded.ledger_commitment_hex,
               ledger_borsh_hex = excluded.ledger_borsh_hex,
               client_requesters_json = excluded.client_requesters_json,
               kind = excluded.kind,
               updated_at_ms = excluded.updated_at_ms",
            params![
                row.row_key,
                row.last_block,
                row.score_milli,
                row.ledger_commitment_hex,
                row.ledger_borsh_hex,
                clients_json,
                row.kind,
                row.updated_at_ms,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn reputation_row(&self, row_key: &str) -> Result<Option<ReputationRow>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT row_key, last_block, score_milli, ledger_commitment_hex, ledger_borsh_hex, client_requesters_json, kind, updated_at_ms
                 FROM reputation_rows WHERE row_key = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![row_key])
            .map_err(|e| e.to_string())?;
        if let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let clients_json: String = r.get(5).map_err(|e| e.to_string())?;
            let clients: Vec<String> =
                serde_json::from_str(&clients_json).unwrap_or_default();
            return Ok(Some(ReputationRow {
                row_key: r.get(0).map_err(|e| e.to_string())?,
                last_block: r.get(1).map_err(|e| e.to_string())?,
                score_milli: r.get(2).map_err(|e| e.to_string())?,
                ledger_commitment_hex: r.get(3).map_err(|e| e.to_string())?,
                ledger_borsh_hex: r.get(4).map_err(|e| e.to_string())?,
                client_requesters_hex: clients,
                kind: r.get(6).map_err(|e| e.to_string())?,
                updated_at_ms: r.get(7).map_err(|e| e.to_string())?,
            }));
        }
        Ok(None)
    }

    pub fn reputation_rows(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReputationRow>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT row_key, last_block, score_milli, ledger_commitment_hex, ledger_borsh_hex, client_requesters_json, kind, updated_at_ms
                 FROM reputation_rows ORDER BY last_block DESC, row_key ASC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit, offset], |r| {
                let clients_json: String = r.get(5)?;
                let clients: Vec<String> = serde_json::from_str(&clients_json).unwrap_or_default();
                Ok(ReputationRow {
                    row_key: r.get(0)?,
                    last_block: r.get(1)?,
                    score_milli: r.get(2)?,
                    ledger_commitment_hex: r.get(3)?,
                    ledger_borsh_hex: r.get(4)?,
                    client_requesters_hex: clients,
                    kind: r.get(6)?,
                    updated_at_ms: r.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn payload_value(row: &TxRow) -> Value {
        serde_json::from_str(&row.payload_json).unwrap_or(Value::Null)
    }
}

fn normalize_hash_lookup(s: &str) -> String {
    let t = s.trim();
    if t.starts_with("0x") || t.starts_with("0X") {
        t.to_string()
    } else {
        format!("0x{t}")
    }
}

fn normalize_address_lookup(s: &str) -> Result<String, String> {
    let mut t = s.trim().to_string();
    if !t.starts_with("0x") && t.len() == 40 {
        t = format!("0x{t}");
    }
    if t.len() != 42 || !t.starts_with("0x") {
        return Err("invalid address".into());
    }
    Ok(t.to_lowercase())
}

fn is_tx_hash_query(s: &str) -> bool {
    let t = s.strip_prefix("0x").unwrap_or(s);
    t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_block_number(s: &str) -> Option<u64> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("latest") {
        return None;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok();
    }
    t.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_roundtrip_block_and_tx() {
        let dir = tempfile::tempdir().unwrap();
        let db = IndexerDb::open(&dir.path().join("t.db")).unwrap();
        db.insert_block(&BlockRow {
            number: 7,
            hash: "0xblock".into(),
            timestamp_ms: 1_700_000_000_000,
            tx_count: 1,
        })
        .unwrap();
        let b = db.block(7).unwrap().unwrap();
        assert_eq!(b.hash, "0xblock");
        db.insert_tx(
            &TxRow {
                hash: "0xtx".into(),
                block_number: 7,
                tx_index: 0,
                signer: "0xs".into(),
                vm_kind: "Native".into(),
                call_kind: Some("WalletCloseBudgetAccountV1".into()),
                payload_json: r#"{"type":"WalletCloseBudgetAccountV1","budget":1}"#.into(),
                receipt_status: Some(1),
                gas_used: Some(21_000),
                transfer_to: None,
            },
            true,
        )
        .unwrap();
        let tx = db.transaction("0xtx").unwrap().unwrap();
        assert_eq!(tx.call_kind.as_deref(), Some("WalletCloseBudgetAccountV1"));
        let st = db.status().unwrap();
        assert_eq!(st.tx_count, 1);
        assert_eq!(st.wallet_event_count, 1);
        assert_eq!(st.reputation_row_count, 0);
    }
}
