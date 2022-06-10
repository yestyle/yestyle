use anyhow::Result;
use std::{fs::File, io::Write};

const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/octokit/graphql-schema/master/schema.graphql";
const SCHEMA_FILE: &str = "graphql/github_schema.graphql";

#[tokio::main]
async fn main() -> Result<()> {
    let content = reqwest::get(SCHEMA_URL).await?.text().await?;
    let mut file = File::create(SCHEMA_FILE)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
