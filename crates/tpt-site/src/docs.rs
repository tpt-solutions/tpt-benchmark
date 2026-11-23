//! Renders a directory of Markdown files as site pages.
//!
//! Raw HTML inside the Markdown is escaped, not passed through, because the
//! tool may be pointed at documents from any repository.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

use crate::site::slugify;

/// One rendered Markdown file.
#[derive(Clone, Debug)]
pub struct DocPage {
    /// File stem used for the page name, `docs/<slug>.html`.
    pub slug: String,
    pub title: String,
    pub html: String,
}

/// GitHub-style heading id: lowercase, spaces to `-`, punctuation dropped.
fn heading_id(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else if c == ' ' {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// Resolves `rel` against `base_dir` (both `/`-separated). `None` when it
/// climbs out of the repository.
fn resolve(base_dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = base_dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Rewrites a link found in a doc so it still works on the site.
fn rewrite_link(dest: &str, repo: Option<&str>) -> String {
    if dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with('#') {
        return dest.to_owned();
    }
    let (path, frag) = match dest.split_once('#') {
        Some((p, f)) => (p, format!("#{f}")),
        None => (dest, String::new()),
    };
    if let Some(stem) = path.strip_suffix(".md") {
        if !stem.contains('/') {
            return format!("{}.html{frag}", slugify(stem));
        }
    }
    // Some other file in the repository: point at the repository, if known.
    match (repo, resolve("docs", path)) {
        (Some(repo), Some(resolved)) if repo.starts_with("https://github.com/") => {
            format!("{repo}/blob/HEAD/{resolved}{frag}")
        }
        _ => dest.to_owned(),
    }
}

/// Converts Markdown to an HTML fragment. Returns the first level-1 heading
/// as the title, if there is one.
pub fn render_markdown(markdown: &str, repo: Option<&str>) -> (Option<String>, String) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let events: Vec<Event<'_>> = Parser::new_ext(markdown, options).collect();

    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut title = None;
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading { level, .. }) => {
                let mut text = String::new();
                let mut j = i + 1;
                while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                    if let Event::Text(t) | Event::Code(t) = &events[j] {
                        text.push_str(t);
                    }
                    j += 1;
                }
                if *level == HeadingLevel::H1 && title.is_none() {
                    title = Some(text.clone());
                }
                let base = heading_id(&text);
                let count = seen.entry(base.clone()).or_insert(0);
                let id = if *count == 0 {
                    base
                } else {
                    format!("{base}-{count}")
                };
                *count += 1;
                let n = *level as usize;
                out.push(Event::Html(CowStr::from(format!("<h{n} id=\"{id}\">"))));
                out.extend(events[i + 1..j].iter().cloned().map(neutralise));
                out.push(Event::Html(CowStr::from(format!("</h{n}>\n"))));
                i = j + 1;
                continue;
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title: link_title,
                id,
            }) => out.push(Event::Start(Tag::Link {
                link_type: *link_type,
                dest_url: CowStr::from(rewrite_link(dest_url, repo)),
                title: link_title.clone(),
                id: id.clone(),
            })),
            other => out.push(neutralise(other.clone())),
        }
        i += 1;
    }

    let mut body = String::new();
    html::push_html(&mut body, out.into_iter());
    (title, body)
}

/// Turns raw HTML in the source into visible text, which `push_html` escapes.
fn neutralise(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        other => other,
    }
}

/// Renders every `.md` file directly inside `dir`, sorted by name.
pub fn collect(dir: &Path, repo: Option<&str>) -> io::Result<Vec<DocPage>> {
    let mut pages = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let (title, html) = render_markdown(&fs::read_to_string(&path)?, repo);
        let mut slug = slugify(stem);
        if slug == "index" {
            slug = "index-page".to_owned();
        }
        pages.push(DocPage {
            title: title.unwrap_or_else(|| stem.to_owned()),
            slug,
            html,
        });
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_get_github_style_ids_and_the_first_h1_is_the_title() {
        let (title, html) = render_markdown(
            "# Behaviour: the caps\n\n## Why `x`?\n\n## Why `x`?\n",
            None,
        );
        assert_eq!(title.as_deref(), Some("Behaviour: the caps"));
        assert!(html.contains("<h1 id=\"behaviour-the-caps\">"));
        assert!(html.contains("<h2 id=\"why-x\">"));
        assert!(html.contains("<h2 id=\"why-x-1\">"));
    }

    #[test]
    fn raw_html_is_escaped() {
        let (_, html) = render_markdown(
            "hi <script>alert(1)</script>\n\n<div onclick=x>y</div>\n",
            None,
        );
        assert!(!html.contains("<script>") && !html.contains("<div"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn tables_and_code_render() {
        let (_, html) = render_markdown(
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n",
            None,
        );
        assert!(html.contains("<table>") && html.contains("language-rust"));
    }

    #[test]
    fn links_are_rewritten() {
        let repo = Some("https://github.com/o/r");
        assert_eq!(
            rewrite_link("statistics.md#caps", repo),
            "statistics.html#caps"
        );
        assert_eq!(rewrite_link("#local", repo), "#local");
        assert_eq!(
            rewrite_link("https://x.dev/a.md", repo),
            "https://x.dev/a.md"
        );
        assert_eq!(
            rewrite_link("../crates/a/src/lib.rs", repo),
            "https://github.com/o/r/blob/HEAD/crates/a/src/lib.rs"
        );
        assert_eq!(rewrite_link("../../escape", repo), "../../escape");
        assert_eq!(rewrite_link("../crates/a.rs", None), "../crates/a.rs");
    }
}
