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
    recent_commits: Vec<ContributedCommit>,
}

#[derive(Serialize)]
struct ContributedCommit {
    repo_owner: String,
    repo_name: String,
    commit_url: URI,
    commit_headline: String,
    commit_date: DateTime,
}

async fn user_contribution_query(
    client: &Client,
    after: Option<String>,
) -> Result<Response<user_contributed_repos_query::ResponseData>> {
    for i in 1..5 {
        let vars = user_contributed_repos_query::Variables {
            login: MY_LOGIN.to_string(),
            email: MY_EMAIL.to_string(),
            after: after.clone(),
        };
        let resp = post_graphql::<UserContributedReposQuery, _>(client, API_URL, vars).await?;
        if let Some(errors) = resp.errors {
            eprintln!(
                "user contribution query attempt #{i}: {}",
                errors[0].message
            );
        } else {
            return Ok(resp);
        }
    }
    panic!("Could not get results for user contribution query after 5 attempts");
}

async fn get_user_recent_commits(client: &Client) -> Result<Vec<ContributedCommit>> {
    let mut commits = Vec::new();
    let mut after = None;

    loop {
        let resp = user_contribution_query(client, after).await?;
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
                Some(user_contributed_repos_query::ReposNodesDefaultBranchRefTarget::Commit(c)) => {
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
                        let committed_date =
                            chrono::DateTime::parse_from_rfc3339(&commit.committed_date)
                                .unwrap_or_else(|e| {
                                    panic!(
                                        "Could not parse '{}' as RFC3339 datetime: {e}",
                                        commit.committed_date
                                    )
                                })
                                .with_timezone(&chrono::Utc)
                                .format(DATE_FORMAT)
                                .to_string();

                        commits.push(ContributedCommit {
                            repo_owner: repo.owner.login.clone(),
                            repo_name: repo.name.clone(),
                            commit_url: commit.commit_url.clone(),
                            commit_headline: commit.message_headline.clone(),
                            commit_date: committed_date,
                        });
                    }
                }
                _ => continue,
            }
        }

        if contributions.page_info.has_next_page {
            after = contributions.page_info.end_cursor;
        } else {
            break;
        }
    }

    commits.sort_by(|a, b| b.commit_date.cmp(&a.commit_date));
    commits.dedup_by(|a, b| a.commit_headline == b.commit_headline);

    Ok(commits)
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

    let recent_commits = get_user_recent_commits(&client).await?;

    let mut tt = TinyTemplate::new();
    tt.add_template("readme", README_TEMPLATE)?;
    let context = Context {
        blog_posts,
        recent_commits,
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

{{ for commit in recent_commits }}- {commit.repo_owner}/{commit.repo_name} - [{commit.commit_headline}]({commit.commit_url}) - {commit.commit_date}
{{ endfor }}

"#;
