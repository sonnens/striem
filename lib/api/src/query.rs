use duckdb::Connection;

use anyhow::Result;
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Serialize)]
pub struct QueryResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    #[serde(rename = "rowCount")]
    pub row_count: usize,
    #[serde(rename = "executionTime")]
    pub execution_time: u128,
}

#[derive(Serialize)]
pub struct QueryError {
    pub error: String,
}

pub fn execute_query(sql: &str, limit: usize) -> Result<serde_json::Value> {
    let conn = Connection::open_in_memory()?;
    conn.execute(
        r#"SET allowed_directories TO ['/Users/john/Desktop/code/striem-test/olddata',
                                                     'application_activity',
                                                     'discovery',
                                                     'findings',
                                                     'identity_access_management',
                                                     'iam',
                                                     'network_activity',
                                                     'remediation',
                                                     'system_activity'];
                         SET file_search_path TO '/Users/john/Desktop/code/striem-test/olddata/';
                         SET parquet_metadata_cache TO true;
                         SET enable_external_access TO false;"#,
        [],
    )?;

    let sql = if !sql.trim().to_lowercase().contains("limit") {
        format!("{} LIMIT {}", sql.trim_end_matches(';'), limit)
    } else {
        sql.to_string()
    };

    let mut stmt = conn.prepare(&sql)?;
    let res = stmt.query_arrow([])?;
    let out: Vec<_> = res.collect::<Vec<_>>();

    let buf = Vec::new();
    let mut writer = arrow_json::writer::ArrayWriter::new(buf);
    let batch_refs: Vec<&_> = out.iter().collect();
    writer.write_batches(&batch_refs).unwrap();
    writer.finish().unwrap();
    Ok(serde_json::from_reader(writer.into_inner().as_slice())?)
}
