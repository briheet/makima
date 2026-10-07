//! Domain types as `TEXT` columns: stored through `Display`, read back
//! through `FromStr`. A value that doesn't parse is a database error, not a
//! silently missing row.

use std::fmt::Display;
use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// `T` as a `TEXT` column. A local wrapper, since neither `rusqlite`'s
/// traits nor Makima's types belong to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Text<T>(pub T);

impl<T: Display> ToSql for Text<T> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

impl<T> FromSql for Text<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map(Text)
            .map_err(|e| FromSqlError::Other(Box::new(e)))
    }
}

/// An optional `T` column: `NULL` is `None`.
pub(crate) fn text<T: Display>(value: Option<&T>) -> Option<Text<&T>> {
    value.map(Text)
}

/// Reads column `index` as `T`.
pub(crate) fn get<T>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    row.get::<_, Text<T>>(index).map(|t| t.0)
}

/// Reads nullable column `index` as `T`.
pub(crate) fn get_opt<T>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    row.get::<_, Option<Text<T>>>(index).map(|t| t.map(|t| t.0))
}
