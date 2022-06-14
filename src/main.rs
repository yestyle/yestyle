use anyhow::Result;
use graphql_client::{reqwest::post_graphql, GraphQLQuery, Response};
use reqwest::Client;
use rss::Channel;
use serde_derive::Serialize;
use std::{env, fs::File, io::Write, path::PathBuf};
use tinytemplate::TinyTemplate;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const MY_LOGIN: &str = "yestyle";
const MY_EMAIL: &str = "yestyle@gmail.com";

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
type DateTime = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github_schema.graphql",
    query_path = "graphql/github_queries.graphql",
    response_derives = "Debug"
)]
struct UserContributedReposQuery;

#[derive(Serialize)]
struct Context {
    blog_posts: Vec<BlogPost>,
    contributed_commits: Vec<UserContributedReposStats>,
}

#[derive(Serialize)]
struct GitCommit {
    url: URI,
    message: String,
    date: DateTime,
}

#[derive(Default, Serialize)]
struct UserContributedReposStats {
    repo_owner: String,
    repo_name: String,
    commits: Vec<GitCommit>,
}

async fn user_query(
    client: &Client,
    after: Option<String>,
) -> Result<Response<<UserContributedReposQuery as GraphQLQuery>::ResponseData>> {
    for i in 1..5 {
        let vars = user_contributed_repos_query::Variables {
            login: MY_LOGIN.to_string(),
            email: MY_EMAIL.to_string(),
            after: after.clone(),
        };
        let resp = post_graphql::<UserContributedReposQuery, _>(client, API_URL, vars).await?;
        if let Some(errors) = resp.errors {
            eprintln!("user query attempt #{i}: {}", errors[0].message);
        } else {
            return Ok(resp);
        }
    }
    panic!("Could not get results for user query after 5 attempts");
}

async fn get_user_contributed_commits(client: &Client) -> Result<Vec<UserContributedReposStats>> {
    let mut stats = Vec::new();
    let mut after = None;

    loop {
        let resp = user_query(client, after).await?;
        let contributions = resp
            .data
            .unwrap_or_else(|| panic!("Response for user repos has no data"))
            .user
            .unwrap_or_else(|| panic!("Response data for user repos has no user"))
            .repositories_contributed_to;

        for repo in contributions
            .nodes
            .expect("Contributions response has no nodes")
            .into_iter()
            .flatten()
        {
            if repo.name == MY_LOGIN {
                continue;
            }
            let mut commits = Vec::new();
            match repo
                .default_branch_ref
                .unwrap_or_else(|| {
                    panic!(
                        "Could not get default branch ref for repo {}",
                        repo.name_with_owner
                    )
                })
                .target
            {
                Some(user_contributed_repos_query::UserContributedReposQueryUserRepositoriesContributedToNodesDefaultBranchRefTarget::Commit(c)) => {
                    let nodes = c.history.nodes.unwrap_or_else(|| {
                        panic!(
                            "Could not get history nodes for repo {}",
                            repo.name_with_owner
                        )
                    });
                    if nodes.is_empty() {
                        continue;
                    }
                    for node in nodes.iter() {
                        let commit = node.as_ref().unwrap_or_else(|| {
                            panic!(
                                "Could not get commit node for repo {}",
                                repo.name_with_owner
                            )
                        });
                        let committed_date = chrono::DateTime::parse_from_rfc3339(&commit.committed_date)
                        .unwrap_or_else(|e| {
                            panic!("Could not parse '{}' as RFC3339 datetime: {e}", commit.committed_date)
                        })
                        .with_timezone(&chrono::Utc)
                        .format(DATE_FORMAT)
                        .to_string();

                        commits.push(GitCommit { url: commit.commit_url.clone(), message: commit.message_headline.clone(), date: committed_date });
                    }
                }
                _ => continue,
            }
            stats.push(UserContributedReposStats {
                repo_owner: repo.owner.login,
                repo_name: repo.name,
                commits,
            })
        }

        if contributions.page_info.has_next_page {
            after = contributions.page_info.end_cursor;
        } else {
            break;
        }
    }

    Ok(stats)
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

    let contributed_commits = get_user_contributed_commits(&client).await?;

    let mut tt = TinyTemplate::new();
    tt.add_template("readme", README_TEMPLATE)?;
    let context = Context {
        blog_posts,
        contributed_commits,
    };

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
            let dt = chrono::DateTime::parse_from_rfc2822(
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

## Recent Commits

{{ for repo in contributed_commits }}{{ for commit in repo.commits }}- {repo.repo_owner}/{repo.repo_name} - [{commit.message}]({commit.url}) - {commit.date}
{{ endfor }}{{ endfor }}

"#;
