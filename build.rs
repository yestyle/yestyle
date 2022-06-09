use anyhow::Result;
use std::{fs::File, io::copy};

#[allow(dead_code)]
const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/octokit/graphql-schema/master/schema.graphql";

#[tokio::main]
async fn main() -> Result<()> {
    let response = reqwest::get(SCHEMA_URL).await?;
    let content = response.text().await?;

    let mut dest = File::create("graphql/github_schema.graphql")?;
    copy(&mut content.as_bytes(), &mut dest)?;
    Ok(())
}
