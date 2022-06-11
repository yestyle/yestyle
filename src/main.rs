use anyhow::Result;
use chrono::DateTime;
use graphql_client::{reqwest::post_graphql, GraphQLQuery};
use reqwest::Client;
use rss::Channel;
use serde_derive::Serialize;
use std::{env, fs::File, io::Write, path::PathBuf};
use tinytemplate::TinyTemplate;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const DATE_FORMAT: &str = "%Y-%m-%d";

const API_URL: &str = "https://api.github.com/graphql";

#[derive(Serialize)]
struct BlogPost {
    title: String,
    date: String,
    url: String,
}

#[allow(clippy::upper_case_acronyms)]
type URI = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github_schema.graphql",
    query_path = "graphql/github_queries.graphql",
    response_derives = "Debug"
)]
struct RepoView;

#[derive(Serialize)]
struct Context {
    blog_posts: Vec<BlogPost>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let blog_posts = blog_posts().await?;

    let token = env::var("GITHUB_TOKEN")
        .expect("You must set the GITHUB_TOKEN env var when running this program");
    let bearer = format!("Bearer {}", token);
    let client = Client::builder()
        .user_agent(format!("github-readme-generator/{}", VERSION))
        .default_headers(
            std::iter::once((
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&bearer)
                    .unwrap_or_else(|e| panic!("Could not parse header from '{bearer}': {e}")),
            ))
            .collect(),
        )
        .build()?;

    let variables = repo_view::Variables {
        owner: "yestyle".into(),
        name: "yestyle".into(),
    };

    let response_body = post_graphql::<RepoView, _>(&client, API_URL, variables).await?;

    println!("{:?}", response_body);

    let mut tt = TinyTemplate::new();
    tt.add_template("readme", README_TEMPLATE)?;
    let context = Context { blog_posts };

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("README.md");
    let mut file = File::create(path)?;
    file.write_all(tt.render("readme", &context)?.as_bytes())?;

    Ok(())
}

async fn blog_posts() -> Result<Vec<BlogPost>> {
    let content = reqwest::get("https://blog.lancitou.net/feed")
        .await?
        .bytes()
        .await?;
    let channel = Channel::read_from(&content[..])?;
    channel
        .items
        .into_iter()
        .filter(|i| i.author().is_some() && i.author().unwrap() == "Philip Ye")
        .map(|i| {
            let title = i
                .title()
                .unwrap_or_else(|| panic!("Blog post has no title"));
            let dt = DateTime::parse_from_rfc2822(
                i.pub_date()
                    .as_ref()
                    .unwrap_or_else(|| panic!("Blog post '{title}', has no publication date")),
            )?;
            Ok(BlogPost {
                title: title.to_string(),
                date: dt.date().format(DATE_FORMAT).to_string(),
                url: i
                    .link()
                    .unwrap_or_else(|| panic!("Blog post '{title}', has no link"))
                    .to_string(),
            })
        })
        .collect::<Vec<_>>()
        .splice(0..5, None)
        .collect::<Result<Vec<_>>>()
}

const README_TEMPLATE: &str = r#"
# Philip Ye

This file was generated by the Rust program in
https://github.com/yestyle/yestyle, which cloned and modified from
https://github.com/autarch/autarch.

## Recent Blog Posts

{{ for post in blog_posts }}- [{post.title}]({post.url}) - {post.date}
{{ endfor }}

"#;
