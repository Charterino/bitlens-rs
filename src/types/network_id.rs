use std::fmt::Display;

use deadpool_sqlite::rusqlite::{
    Result, ToSql,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef},
};
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};

#[derive(FromPrimitive, ToPrimitive, Clone, Default, Debug)]
pub enum NetworkId {
    #[default]
    Error = 0,
    IPv4 = 1,
    IPv6 = 2,
    Onionv2 = 3,
    Onionv3 = 4,
    I2P = 5,
    CJDNS = 6,
    YggDrasil = 7,
}

impl FromSql for NetworkId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match NetworkId::from_i64(value.as_i64()?) {
            Some(a) => Ok(a),
            None => Err(FromSqlError::InvalidType),
        }
    }
}

impl ToSql for NetworkId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(self.to_i64().unwrap())))
    }
}

impl Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            NetworkId::Error => f.write_str("error"),
            NetworkId::IPv4 => f.write_str("IPv4"),
            NetworkId::IPv6 => f.write_str("IPv6"),
            NetworkId::Onionv2 => f.write_str("Onionv2"),
            NetworkId::Onionv3 => f.write_str("Onionv3"),
            NetworkId::I2P => f.write_str("I2P"),
            NetworkId::CJDNS => f.write_str("CJDNS"),
            NetworkId::YggDrasil => f.write_str("YggDrasil"),
        }
    }
}
