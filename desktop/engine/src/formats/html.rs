//! HTML to Markdown (tables, lists, links, code).

use anyhow::{anyhow, Result};
use htmd::options::{BulletListMarker, Options};

pub fn convert(html: &str) -> Result<String> {
    let conv = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "noscript", "head", "iframe", "svg", "nav", "form", "template"])
        .options(Options {
            bullet_list_marker: BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ol_number_spacing: 1,
            ..Options::default()
        })
        .build();
    conv.convert(html).map_err(|e| anyhow!("Could not parse HTML: {e}"))
}
