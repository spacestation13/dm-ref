use std::{
    collections::{HashMap, HashSet},
    fmt::Write as FmtWrite,
    fs::{self, create_dir_all, File},
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use clap::Parser;
use regex::Regex;
use scraper::{Html, Selector};
use toml_edit::{value, Array, Table};

static PAGE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a").unwrap());
static TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("h2").unwrap());
static BODY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body").unwrap());
static DL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("dl").unwrap());
static B_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("b").unwrap());
static DT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("dt").unwrap());
static DD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("dd").unwrap());
static A_LINK_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a[href]").unwrap());

static PROC_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:procs)|(?:vars) \((.*)\)").unwrap());
static PROC_VAR_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(.*) (?:proc)|(?:var)").unwrap());
static CODE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("<(/)?(tt|code)>").unwrap());
static ORPHAN_TT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)tt>").unwrap());
static LINK_BACKSLASH_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("(`.*\\.*`)").unwrap());
static CODE_PERCENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("`(.*)(%%)(.*)`").unwrap());
static NAIVE_STRIPPER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("<a name.*?>.*?</a>").unwrap());
static CLEAN_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("[{}]").unwrap());

const TEXT_REPLACEMENTS: &[(char, &str)] = &[
    ('.', "dot"),
    ('<', "greater"),
    ('>', "less"),
    ('%', "modulo"),
    ('?', "query"),
    ('&', "amp"),
    ('~', "tilde"),
    ('|', "vert"),
    ('!', "exclaim"),
    (':', "colon"),
    ('*', "asterisk"),
    ('^', "caret"),
    ('=', "equals"),
    ('+', "plus"),
    ('(', "leftparen"),
    (')', "rightparen"),
    ('[', "leftsquare"),
    (']', "rightsquare"),
];

#[derive(Parser)]
#[command(name = "dm-ref-scraper")]
#[command(about = "Converts BYOND DreamMaker reference HTML to Markdown with TOML frontmatter")]
struct Args {
    /// Path to input HTML file
    #[arg(long = "ref", default_value = "info.html")]
    input: PathBuf,

    /// Output directory
    #[arg(long, default_value = "build")]
    output: PathBuf,
}

#[derive(Debug)]
struct Page {
    title: String,
    body: String,
    metadata: Vec<(String, Vec<String>)>,
    version: Option<String>,
    tags: Vec<String>,
}

impl Page {
    fn to_frontmatter(&self, is_object: bool) -> String {
        let mut page_toml = toml_edit::DocumentMut::new();

        // Quartz will choke on double-ampersands, but only in the title field
        page_toml["title"] = value(self.title.replace("%%", r"\%\%"));

        if let Some(version) = &self.version {
            page_toml["byond_version"] = value(version);
        }

        let mut tags = Array::from_iter(self.tags.iter());
        if is_object {
            tags.push("object");
        }

        let mut headers = Table::new();
        for item in &self.metadata {
            headers[&item.0] = Array::from_iter(item.1.iter()).into();
        }

        page_toml["headers"] = headers.into();
        page_toml["tags"] = tags.into();

        page_toml.to_string()
    }
}

fn main() {
    let args = Args::parse();

    let Ok(raw) = fs::read_to_string(&args.input) else {
        eprintln!("Failed to read input file: {:?}", args.input);
        std::process::exit(1);
    };

    eprintln!("Read {} bytes from {:?}", raw.len(), args.input);

    let parts: Vec<&str> = raw.split("<hr>").collect();
    eprintln!("Split into {} parts", parts.len());

    let mut path_to_doc: HashMap<String, Html> = HashMap::new();
    let mut page_is_section: HashSet<String> = HashSet::new();

    let mut skipped_no_anchor = 0;
    let mut skipped_no_name = 0;

    for page in parts.iter() {
        let document = Html::parse_document(page);

        let Some(page_element) = document.select(&PAGE_SELECTOR).next() else {
            skipped_no_anchor += 1;
            continue;
        };

        let Some(page_path) = page_element.attr("name") else {
            skipped_no_name += 1;
            continue;
        };

        if let Some(parent) = Path::new(page_path).parent().and_then(|p| p.to_str()) {
            page_is_section.insert(parent.to_string());
        }

        path_to_doc.insert(page_path.to_string(), document);
    }

    eprintln!(
        "Parsed {} documents (skipped: {} no anchor, {} no name attr)",
        path_to_doc.len(),
        skipped_no_anchor,
        skipped_no_name
    );

    let mut path_to_page: HashMap<String, Page> = HashMap::new();
    let mut page_is_object: HashSet<String> = HashSet::new();

    for (page_path, document) in &path_to_doc {
        create_page_from_html(
            page_path,
            document,
            &mut path_to_page,
            &path_to_doc,
            &mut page_is_object,
        );
    }

    path_to_page.insert(
        "/".to_string(),
        Page {
            title: "Reference".to_string(),
            body: "# dm-ref-scraper and Quartz

This site is made using [Quartz](https://quartz.jzhao.xyz/) and [dm-ref-scraper](https://github.com/spacestation13/dm-ref-scraper). You probably want to start [here](/DM)!
    "
            .to_string(),
            version: None,
            tags: Vec::new(),
            metadata: Vec::new(),
        },
    );

    eprintln!("Writing {} pages to {:?}", path_to_page.len(), args.output);

    let mut written = 0;
    let mut failed = 0;

    for (path, page) in &path_to_page {
        let mut path_str = make_ref_web_safe(path);

        if page_is_section.contains(path) {
            path_str = format!("{}/index", path_str);
        }

        let clean_path = format!("{}{}", args.output.display(), &path_str);
        let file_path = Path::new(&clean_path);
        let prefix = file_path.parent().unwrap();
        create_dir_all(prefix).unwrap();

        let Ok(mut file) = File::create(format!("{}.md", clean_path)) else {
            eprintln!("Failed to create file: {}.md", clean_path);
            failed += 1;
            continue;
        };

        let body = remove_html_encode(&page.body);
        let body = CODE_REGEX.replace_all(&body, "`").to_string();
        let body = ORPHAN_TT_REGEX.replace_all(&body, "").to_string();
        let body = escape_percents(&body);
        let body = escape_dollars_outside_code(&body);

        let frontmatter = page.to_frontmatter(page_is_object.contains(&page.title));
        let front_matter_and_body = format!("+++\n{}+++\n{}", frontmatter, body);

        if let Err(e) = file.write_all(front_matter_and_body.as_bytes()) {
            eprintln!("Failed to write {}.md: {}", clean_path, e);
            failed += 1;
        } else {
            written += 1;
        }
    }

    eprintln!("Done: {} written, {} failed", written, failed);
}

fn create_page_from_html(
    page_path: &str,
    document: &Html,
    path_to_page: &mut HashMap<String, Page>,
    path_to_doc: &HashMap<String, Html>,
    page_is_object: &mut HashSet<String>,
) {
    let title_element = document.select(&TITLE_SELECTOR).next().unwrap();
    let title = title_element.inner_html();

    let mut tags: Vec<String> = Vec::new();

    if title.contains(" proc") {
        tags.push("proc".to_string());
    }

    if title.contains(" var") {
        tags.push("var".to_string());
    }

    let target_name = PROC_VAR_NAME_REGEX
        .captures(&title)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_owned());

    if let Some(operator) = PROC_VAR_REGEX.captures(&title).and_then(|cap| cap.get(1)) {
        page_is_object.insert(operator.as_str().to_string());
    }

    // First pass: extract bold-header <dl> blocks for frontmatter and top-of-page rendering
    let mut headers: Vec<(String, Vec<String>, bool)> = Vec::new();
    let mut bold_dl_ids: HashSet<ego_tree::NodeId> = HashSet::new();
    for data_part in document.select(&DL_SELECTOR) {
        let Some(data_title_element) = data_part.select(&DT_SELECTOR).next() else {
            continue;
        };

        if data_title_element.select(&B_SELECTOR).next().is_none() {
            continue;
        }

        bold_dl_ids.insert(data_part.id());

        let bold_element = data_title_element.select(&B_SELECTOR).next().unwrap();
        let data_title = bold_element.inner_html().replace(':', "");

        if data_title.contains("When") {
            tags.push("event".to_string());
        }

        let mut opt_array: Vec<String> = Vec::new();
        for results in data_part.select(&DD_SELECTOR) {
            let mut stripped = results.inner_html();

            stripped = parse_html_to_markdown(
                NAIVE_STRIPPER_REGEX.replace_all(&stripped, "").to_string(),
                path_to_doc,
            );
            if stripped.is_empty() {
                continue;
            }

            opt_array.push(stripped);
        }

        let is_code_header = data_part
            .value()
            .has_class("codedd", scraper::CaseSensitivity::CaseSensitive)
            || data_title == "Format";

        headers.push((data_title, opt_array, is_code_header));
    }

    // Render bold headers at top of page
    let mut text: Vec<String> = Vec::new();
    let mut write_after: Vec<String> = Vec::new();
    for part in &headers {
        let mut to_write = String::new();
        let _ = write!(to_write, "### {}", part.0);

        if part.1.len() > 1 {
            to_write.push('\n');

            for string in &part.1 {
                if part.0 == "Args" && string.contains(':') {
                    let split: Vec<&str> = string.split(':').collect();
                    let _ = write!(to_write, "- `{}`:{}", split[0], split[1]);
                } else {
                    if part.2 && !string.starts_with('[') {
                        let _ = write!(to_write, "- `{}`", string);
                    } else {
                        let _ = write!(to_write, "- {}", string);
                    }
                }

                to_write.push('\n');
            }
        } else if let Some(wrap) = part.1.first() {
            if part.2 {
                let _ = write!(to_write, "\n> `{}`", wrap);
            } else {
                let _ = write!(to_write, "\n> {}", wrap);
            }
        }

        if part.0 == "See also" || part.0.contains("/var") || part.0.contains("/proc") {
            write_after.push(clean_code_backslashes(&clean_code_percentage(&to_write)));
        } else {
            text.push(clean_code_backslashes(&clean_code_percentage(&to_write)));
        }
    }

    // Second pass: iterate body children in document order
    // Flatten name-only <a> anchors so their children are processed inline
    let body = document.select(&BODY_SELECTOR).next().unwrap();
    let block_elements = [
        "dl", "p", "h3", "xmp", "pre", "ul", "table", "div", "hr", "ol",
    ];
    let skip_elements = ["h2"];
    let mut inline_accumulator = String::new();

    let flush_inline =
        |acc: &mut String, text: &mut Vec<String>, all_pages: &HashMap<String, Html>| {
            if !acc.trim().is_empty() {
                let md = parse_html_to_markdown(acc.clone(), all_pages);
                if !md.trim().is_empty() {
                    text.push(md.trim().to_string());
                }
            }
            acc.clear();
        };

    let mut content_nodes: Vec<ego_tree::NodeRef<scraper::Node>> = Vec::new();
    for child in body.children() {
        if let scraper::Node::Element(elem) = child.value() {
            let name = elem.name.local.as_ref();
            if name == "a" && elem.attr("href").is_none() {
                content_nodes.extend(child.children());
                continue;
            }
        }
        content_nodes.push(child);
    }

    for child in &content_nodes {
        match child.value() {
            scraper::Node::Text(t) => {
                inline_accumulator.push_str(t);
                continue;
            }
            scraper::Node::Element(elem) => {
                let name = elem.name.local.as_ref();
                let element = scraper::ElementRef::wrap(*child).unwrap();

                if skip_elements.contains(&name) {
                    continue;
                }

                if !block_elements.contains(&name) {
                    inline_accumulator.push_str(&element.html());
                    continue;
                }

                flush_inline(&mut inline_accumulator, &mut text, path_to_doc);

                match name {
                    "dl" => {
                        if bold_dl_ids.contains(&child.id()) {
                            continue;
                        }

                        if elem.has_class("codedt", scraper::CaseSensitivity::CaseSensitive) {
                            let mut definition_list = String::new();
                            let dt_elements: Vec<_> = element.select(&DT_SELECTOR).collect();
                            let dd_elements: Vec<_> = element.select(&DD_SELECTOR).collect();

                            for (dt, dd) in dt_elements.iter().zip(dd_elements.iter()) {
                                let term = parse_html_to_markdown(dt.inner_html(), path_to_doc);
                                let description =
                                    parse_html_to_markdown(dd.inner_html(), path_to_doc);
                                let _ = writeln!(
                                    definition_list,
                                    "- **{}**: {}",
                                    term.trim(),
                                    description.trim()
                                );
                            }

                            if !definition_list.is_empty() {
                                text.push(definition_list.trim().to_string());
                            }
                        } else {
                            let dt_elements: Vec<_> = element.select(&DT_SELECTOR).collect();
                            let dd_elements: Vec<_> = element.select(&DD_SELECTOR).collect();

                            for (dt, dd) in dt_elements.iter().zip(dd_elements.iter()) {
                                let term = parse_html_to_markdown(
                                    dt.inner_html().replace(':', ""),
                                    path_to_doc,
                                )
                                .trim()
                                .to_string();

                                if term.contains("When") {
                                    tags.push("event".to_string());
                                }

                                let body = render_dd_content(dd, &target_name, path_to_doc);
                                if !body.is_empty() {
                                    let quoted = body
                                        .lines()
                                        .map(|line| {
                                            if line.is_empty() {
                                                ">".to_string()
                                            } else {
                                                format!("> {}", line)
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    text.push(format!("**{}**\n\n{}", term, quoted));
                                }
                            }
                        }
                    }
                    "p" => {
                        if elem.has_class("note", scraper::CaseSensitivity::CaseSensitive)
                            || element.inner_html().starts_with("Note:")
                        {
                            let mut note_type = "note";

                            if elem.has_class("deprecated", scraper::CaseSensitivity::CaseSensitive)
                            {
                                note_type = "deprecated";
                            };

                            if elem.has_class("security", scraper::CaseSensitivity::CaseSensitive) {
                                note_type = "danger";
                            };

                            text.push(format!(
                                "> [!{}]\n> {}",
                                note_type,
                                parse_html_to_markdown(
                                    element.inner_html().replace("Note:", ""),
                                    path_to_doc
                                )
                            ));
                        } else {
                            text.push(parse_html_to_markdown(element.inner_html(), path_to_doc));
                        }
                    }
                    "h3" => {
                        if element.inner_html() == "Example:" {
                            continue;
                        }

                        text.push(format!(
                            "## {}",
                            parse_html_to_markdown(element.inner_html(), path_to_doc)
                        ));
                    }
                    "xmp" => {
                        if let Some(ref target) = target_name {
                            text.push(format!(
                                "```dream-maker /{}/\n{}\n```",
                                target,
                                element.inner_html().trim()
                            ));
                        } else {
                            text.push(format!(
                                "```dream-maker\n{}\n```",
                                element.inner_html().trim()
                            ));
                        }
                    }
                    "pre" => {
                        let has_child_elements = element.children().any(|c| c.value().is_element());
                        if has_child_elements {
                            text.push(render_pre_with_links(&element, path_to_doc));
                        } else {
                            text.push(format!("```\n{}\n```", element.inner_html().trim()));
                        }
                    }
                    "ul" => text.push(parse_html_to_markdown(element.html(), path_to_doc)),
                    "div" | "table" | "ol" => text.push(element.html()),
                    _ => (),
                }
            }
            _ => (),
        }
    }

    flush_inline(&mut inline_accumulator, &mut text, path_to_doc);
    text.extend(write_after);

    let version = title_element
        .attr("byondver")
        .map(|version| version.to_string());

    let mut parsed_metadata = Vec::new();

    for element in &headers {
        if element.0 == "See also" {
            continue;
        }

        parsed_metadata.push((
            element.0.to_owned(),
            element
                .1
                .iter()
                .map(|val| val.replace('\\', "").replace("%%", "\\%\\%"))
                .collect(),
        ));
    }

    path_to_page.insert(
        page_path.to_string(),
        Page {
            title: remove_html_encode(&title),
            body: text.join("\n\n"),
            version,
            tags,
            metadata: parsed_metadata,
        },
    );
}

fn render_pre_with_links(pre: &scraper::ElementRef, all_pages: &HashMap<String, Html>) -> String {
    let mut result = String::new();

    for child in pre.children() {
        match child.value() {
            scraper::Node::Text(t) => {
                let escaped = t
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                result.push_str(&escaped);
            }
            scraper::Node::Element(elem) => {
                let el = scraper::ElementRef::wrap(child).unwrap();
                let name = elem.name.local.as_ref();

                if name == "a" {
                    if let Some(dest) = elem.attr("href") {
                        let final_destination = dest.replace('#', "");
                        let link_text = el.inner_html();
                        if all_pages.contains_key(&final_destination)
                            || final_destination.contains("http")
                        {
                            let _ = write!(
                                result,
                                "<a href=\"{}\">{}</a>",
                                make_ref_web_safe(&final_destination),
                                link_text
                            );
                        } else {
                            result.push_str(&link_text);
                        }
                    } else {
                        result.push_str(&remove_html_encode(&el.text().collect::<String>()));
                    }
                } else {
                    result.push_str(&el.html());
                }
            }
            _ => {}
        }
    }

    format!("<pre>{}</pre>", result.trim())
}

fn render_dd_content(
    dd: &scraper::ElementRef,
    target_name: &Option<String>,
    all_pages: &HashMap<String, Html>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut html_accumulator = String::new();

    let flush_html =
        |acc: &mut String, parts: &mut Vec<String>, all_pages: &HashMap<String, Html>| {
            if !acc.trim().is_empty() {
                let stripped = NAIVE_STRIPPER_REGEX.replace_all(acc, "").to_string();
                let md = parse_html_to_markdown(stripped, all_pages);
                if !md.trim().is_empty() {
                    parts.push(md.trim().to_string());
                }
            }
            acc.clear();
        };

    for child in dd.children() {
        match child.value() {
            scraper::Node::Text(t) => {
                html_accumulator.push_str(t);
            }
            scraper::Node::Element(elem) => {
                let name = elem.name.local.as_ref();
                if name == "xmp" {
                    flush_html(&mut html_accumulator, &mut parts, all_pages);
                    let el = scraper::ElementRef::wrap(child).unwrap();
                    let code = el.inner_html();
                    if let Some(ref target) = target_name {
                        parts.push(format!("```dream-maker /{}/\n{}\n```", target, code.trim()));
                    } else {
                        parts.push(format!("```dream-maker\n{}\n```", code.trim()));
                    }
                } else if name == "p" {
                    flush_html(&mut html_accumulator, &mut parts, all_pages);
                    let el = scraper::ElementRef::wrap(child).unwrap();
                    let stripped = NAIVE_STRIPPER_REGEX
                        .replace_all(&el.inner_html(), "")
                        .to_string();
                    let md = parse_html_to_markdown(stripped, all_pages);
                    if !md.trim().is_empty() {
                        parts.push(md.trim().to_string());
                    }
                } else {
                    let el = scraper::ElementRef::wrap(child).unwrap();
                    html_accumulator.push_str(&el.html());
                }
            }
            _ => {}
        }
    }

    flush_html(&mut html_accumulator, &mut parts, all_pages);
    parts.join("\n\n")
}

fn parse_html_to_markdown(html: String, all_pages: &HashMap<String, Html>) -> String {
    let mut html = html.replace('\n', " ");
    html = CODE_REGEX.replace_all(&html, "`".to_string()).to_string();
    html = ORPHAN_TT_REGEX.replace_all(&html, "").to_string();

    let fragment = Html::parse_fragment(&html);
    for link in fragment.select(&A_LINK_SELECTOR) {
        if let Some(dest) = link.attr("href") {
            let final_destination = dest.replace('#', "");

            if !all_pages.contains_key(&final_destination) && !final_destination.contains("http") {
                html = html.replace(
                    &link.html(),
                    &format!("**BROKEN LINK: {}**", make_ref_web_safe(&final_destination)),
                );
                continue;
            }

            html = html.replace(
                &link.html(),
                &format!(
                    "[{}]({})",
                    link.inner_html(),
                    make_ref_web_safe(&final_destination),
                ),
            );
        }
    }

    html = html2md::parse_html(&html);

    let stripped = NAIVE_STRIPPER_REGEX.replace_all(&html, "").to_string();

    clean_code_backslashes(&clean_code_percentage(&stripped))
}

fn clean_code_percentage(input: &str) -> String {
    CODE_PERCENT_REGEX
        .replace_all(input, r#"`${1}%25%25${3}`"#)
        .to_string()
}

fn clean_code_backslashes(input: &str) -> String {
    let mut cleaning = input.to_string();

    for part in LINK_BACKSLASH_REGEX.captures_iter(input) {
        if let Some(inner) = part.get(1) {
            let inner_string = inner.as_str();
            cleaning = cleaning.replace(inner_string, &inner_string.replace('\\', ""));
        }
    }

    cleaning
}

fn escape_percents(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut remaining = input;
    let mut in_html = false;

    while !remaining.is_empty() {
        if remaining.starts_with('<') {
            in_html = true;
            result.push('<');
            remaining = &remaining[1..];
        } else if in_html && remaining.starts_with('>') {
            in_html = false;
            result.push('>');
            remaining = &remaining[1..];
        } else if remaining.starts_with("%%") {
            if in_html {
                result.push_str("%%");
            } else {
                result.push_str("&#37;&#37;");
            }
            remaining = &remaining[2..];
        } else {
            let c = remaining.chars().next().unwrap();
            result.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
    }

    result
}

fn escape_dollars_outside_code(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut remaining = input;

    while !remaining.is_empty() {
        if remaining.starts_with('`') {
            let backtick_count = remaining.chars().take_while(|&c| c == '`').count();
            let opener = &remaining[..backtick_count];
            remaining = &remaining[backtick_count..];

            if let Some(close_pos) = find_closing_backticks(remaining, backtick_count) {
                result.push_str(opener);
                result.push_str(&remaining[..close_pos + backtick_count]);
                remaining = &remaining[close_pos + backtick_count..];
            } else {
                result.push_str(opener);
            }
        } else if remaining.starts_with('$') {
            result.push_str("\\$");
            remaining = &remaining[1..];
        } else {
            let c = remaining.chars().next().unwrap();
            result.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
    }

    result
}

fn find_closing_backticks(s: &str, count: usize) -> Option<usize> {
    let pattern = "`".repeat(count);
    let mut search_start = 0;

    while let Some(pos) = s[search_start..].find(&pattern) {
        let absolute_pos = search_start + pos;

        let before_ok = absolute_pos == 0 || !s[..absolute_pos].ends_with('`');
        let after_end = absolute_pos + count;
        let after_ok = after_end >= s.len() || !s[after_end..].starts_with('`');

        if before_ok && after_ok {
            return Some(absolute_pos);
        }
        search_start = absolute_pos + 1;
    }
    None
}

fn make_ref_web_safe(dirty_path: &str) -> String {
    let mut path = percent_encoding::percent_decode_str(dirty_path)
        .decode_utf8()
        .unwrap()
        .to_string();

    for replacement in TEXT_REPLACEMENTS {
        path = path.replace(replacement.0, replacement.1);
    }

    path = path.replace("//", "/slash");
    path = path.replace("/index", "/index_page");

    if path.contains("operator") {
        path = path.replace('-', "minus");
    }

    path = CLEAN_REGEX.replace_all(&path, "").to_string();

    path
}

fn remove_html_encode(dirty: &str) -> String {
    dirty
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}
