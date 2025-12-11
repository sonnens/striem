#[cfg(feature = "duckdb")]
pub mod duckdb {
    use anyhow::Result;
    use duckdb::{DuckdbConnectionManager, params};
    use r2d2::PooledConnection;
    use serde::Serialize;
    use serde_json::Value;
    use crate::sources::{Source, ExistingSource};

    const CREATE_TABLE_SQL: &str = 
        r#"CREATE TABLE IF NOT EXISTS sources (
            id UUID PRIMARY KEY,
            config JSON,
            type TEXT);"#;

    pub fn init(db: &mut PooledConnection<DuckdbConnectionManager>) -> Result<()> {
        db.execute(CREATE_TABLE_SQL, [])?;
        Ok(())
    }
    pub fn add_source(db: &mut PooledConnection<DuckdbConnectionManager>, source: &Box<dyn Source>) -> Result<()> {
        let sql = "INSERT INTO sources (type, id, config) VALUES (?, ?, ?, ?)";

        let sourcetype = source.sourcetype();
        let id = source.id();
        let config = source.config().serialize(serde_json::value::Serializer)?;

        db.prepare(sql)?.execute(params![&sourcetype, &id, &config])?;
        Ok(())
    }
    pub fn get_source(db: &mut PooledConnection<DuckdbConnectionManager>, id: String) -> Result<Value> {
        let sql = "SELECT config FROM sources WHERE id = ?";
        let row = db.prepare(sql)?.query_row(params![&id], |row| row.get(0))?;
        Ok(row)
    }

    pub fn get_all_sources(db: &mut PooledConnection<DuckdbConnectionManager>) -> Result<Vec<Box<dyn Source>>> {
        let sql = "SELECT type, id, config FROM sources";

        db.prepare(sql)?
        .query([])?
        .mapped(|row| {
            let sourcetype: String = row.get(0)?;
            let id: String = row.get(1)?;
            let config: Value = row.get(2)?;
            Ok(( sourcetype, id, config ))
        })
        .collect::<Result<Vec<ExistingSource>, duckdb::Error>>()
        .map_err(|e| anyhow::anyhow!("Failed to fetch sources from database: {}", e))?
        .into_iter()
        .map(|row: ExistingSource| {
            row.try_into().map_err(|e| anyhow::anyhow!("Failed to convert source: {}", e))
        })
        .collect::<Result<_, _>>()
    }
}

#[cfg(feature = "duckdb")]
pub use duckdb::*;
