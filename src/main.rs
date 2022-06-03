use anyhow::Result;
use chrono::DateTime;
use rss::Channel;
use serde_derive::Serialize;
use std::{fs::File, io::Write, path::PathBuf};
use tinytemplate::TinyTemplate;

const DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(Serialize)]
struct BlogPost {
    title: String,
    date: String,
    url: String,
}

#[derive(Serialize)]
struct Context {
    blog_posts: Vec<BlogPost>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let blog_posts = blog_posts().await?;

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
    let mut channel = Channel::read_from(&content[..])?;
    channel
        .items
        .splice(0..5, None)
        .into_iter()
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
        .collect::<Result<Vec<_>>>()
}

const README_TEMPLATE: &str = r#"
## Recent Blog Posts

{{ for post in blog_posts }}- [{post.title}]({post.url}) - {post.date}
{{ endfor }}

"#;
