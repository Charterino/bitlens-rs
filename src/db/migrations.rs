pub const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS peers (
  network_id TINYINT NOT NULL,
  address BLOB NOT NULL,
  port SMALLINT NOT NULL,
  first_seen INTEGER,
  /* when we first learned about this peer from some other peer */
  first_online INTEGER,
  /* when we first successfully connected to this peer */
  user_agent TEXT,
  height INTEGER,
  services INTEGER NOT NULL,
  PRIMARY KEY (address, port)
);"#,
    r#"
CREATE INDEX IF NOT EXISTS idx_peers_network_id ON peers (network_id);
"#,
    r#"
CREATE TABLE IF NOT EXISTS banned_peers (
  network_id TINYINT NOT NULL,
  address BLOB NOT NULL,
  port SMALLINT NOT NULL,
  banned_at INTEGER NOT NULL,
  banned_until INTEGER NOT NULL,
  PRIMARY KEY (address, port, banned_until)
)
"#,
    r#"
CREATE INDEX IF NOT EXISTS idx_banned_peers_address_port ON banned_peers (address, port);
"#,
    r#"
CREATE TABLE IF NOT EXISTS headers (
  version INTEGER NOT NULL,
  previous_block BLOB NOT NULL,
  merkle_root BLOB NOT NULL,
  timestamp INTEGER NOT NULL,
  bits INTEGER NOT NULL,
  nonce INTEGER NOT NULL,
  block_number INTEGER NOT NULL,
  block_hash BLOB NOT NULL PRIMARY KEY,
  fetched_full INTEGER NOT NULL DEFAULT 0
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS idx_headers_block_number ON headers (block_number);
"#,
];
