use anyhow::Result;
use chrono::DateTime;
use rss::Channel;

const DATE_FORMAT: &str = "%Y-%m-%d";

#[allow(dead_code)]
struct BlogPost {
    title: String,
    date: String,
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _blog_posts = blog_posts().await?;

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
