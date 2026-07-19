use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Field, Schema, Value, STORED, TEXT},
    Document, Index, IndexReader, IndexWriter, TantivyDocument,
};
use tempfile::TempDir;

mod content {
    include!(concat!(env!("OUT_DIR"), "/content.rs"));
}

const BASE_URL: &str = "https://ref.dm-lang.org";
const EMBED_COLOR: u32 = 0x7160E8;
const MAX_EMBED_TOTAL: usize = 6000;
const MAX_DESCRIPTION: usize = 4096;

struct AppState {
    public_key: VerifyingKey,
    titles_to_path: HashMap<String, &'static str>,
    path_to_parsed: HashMap<String, PageFrontmatter>,
    path_to_text: HashMap<&'static str, &'static str>,
    reader: IndexReader,
    index: Index,
    default_fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Interaction {
    #[serde(rename = "type")]
    interaction_type: u8,
    data: Option<InteractionData>,
}

#[derive(Deserialize)]
struct InteractionData {
    #[allow(dead_code)]
    name: Option<String>,
    options: Option<Vec<CommandOption>>,
    custom_id: Option<String>,
    #[allow(dead_code)]
    component_type: Option<u8>,
    values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CommandOption {
    #[allow(dead_code)]
    name: String,
    value: serde_json::Value,
    #[serde(default)]
    focused: bool,
}

struct FormattedPage {
    title: String,
    path: String,
    url: String,
    footer: String,
    fields: Vec<(String, String)>,
    pages: Vec<String>,
    see_also: Vec<SeeAlsoLink>,
}

struct SeeAlsoLink {
    label: String,
    path: String,
}

async fn handle_interaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let signature_hex = match headers.get("x-signature-ed25519") {
        Some(v) => v.to_str().unwrap_or_default().to_string(),
        None => return (StatusCode::UNAUTHORIZED, "missing signature").into_response(),
    };
    let timestamp = match headers.get("x-signature-timestamp") {
        Some(v) => v.to_str().unwrap_or_default().to_string(),
        None => return (StatusCode::UNAUTHORIZED, "missing timestamp").into_response(),
    };

    let sig_bytes = match hex::decode(&signature_hex) {
        Ok(b) => b,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid signature hex").into_response(),
    };
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid signature").into_response(),
    };

    let mut message = Vec::with_capacity(timestamp.len() + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body.as_bytes());

    if state.public_key.verify(&message, &signature).is_err() {
        return (StatusCode::UNAUTHORIZED, "signature verification failed").into_response();
    }

    let interaction: Interaction = match serde_json::from_str(&body) {
        Ok(i) => i,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid interaction").into_response(),
    };

    let response = match interaction.interaction_type {
        1 => json!({"type": 1}),
        2 => handle_command(&interaction, &state),
        3 => handle_component(&interaction, &state),
        4 => handle_autocomplete(&interaction, &state),
        _ => json!({"type": 1}),
    };

    axum::Json(response).into_response()
}

fn handle_command(interaction: &Interaction, state: &AppState) -> serde_json::Value {
    let search_for = interaction
        .data
        .as_ref()
        .and_then(|d| d.options.as_ref())
        .and_then(|opts| opts.first())
        .and_then(|opt| opt.value.as_str())
        .unwrap_or_default()
        .to_string();

    let search_for = clean_query(search_for);
    let page = get_page(&search_for, state).unwrap_or("Not found.");

    let Some(formatted) = format_page(page, state) else {
        return json!({
            "type": 4,
            "data": {
                "embeds": [{"description": "Could not locate a page."}]
            }
        });
    };

    let embed = build_embed_json(&formatted, 0);
    let components = build_components_json(&formatted, 0, state);

    let mut data = json!({"embeds": [embed]});
    if !components.is_empty() {
        data["components"] = json!(components);
    }

    json!({"type": 4, "data": data})
}

fn handle_component(interaction: &Interaction, state: &AppState) -> serde_json::Value {
    let data = match interaction.data.as_ref() {
        Some(d) => d,
        None => return json!({"type": 7, "data": {}}),
    };

    let custom_id = data.custom_id.as_deref().unwrap_or_default();

    let (path, page_idx) = if custom_id == "s" {
        let selected = data
            .values
            .as_ref()
            .and_then(|v| v.first())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        match get_page(&selected, state) {
            Some(p) => (p, 0usize),
            None => return json!({"type": 7, "data": {}}),
        }
    } else if custom_id.starts_with("p:") || custom_id.starts_with("n:") {
        let parts: Vec<&str> = custom_id.splitn(3, ':').collect();
        if parts.len() < 3 {
            return json!({"type": 7, "data": {}});
        }
        let direction = parts[0];
        let path = parts[1];
        let idx: usize = parts[2].parse().unwrap_or(0);
        let new_idx = if direction == "p" {
            idx.saturating_sub(1)
        } else {
            idx.saturating_add(1)
        };
        (path, new_idx)
    } else {
        return json!({"type": 7, "data": {}});
    };

    let Some(formatted) = format_page(path, state) else {
        return json!({"type": 7, "data": {}});
    };

    let page_idx = page_idx.min(formatted.pages.len().saturating_sub(1));
    let embed = build_embed_json(&formatted, page_idx);
    let components = build_components_json(&formatted, page_idx, state);

    let mut resp_data = json!({"embeds": [embed]});
    if !components.is_empty() {
        resp_data["components"] = json!(components);
    }

    json!({"type": 7, "data": resp_data})
}

fn handle_autocomplete(interaction: &Interaction, state: &AppState) -> serde_json::Value {
    let partial = interaction
        .data
        .as_ref()
        .and_then(|d| d.options.as_ref())
        .and_then(|opts| opts.iter().find(|o| o.focused))
        .and_then(|opt| opt.value.as_str())
        .unwrap_or_default()
        .trim();

    if partial.is_empty() {
        return json!({"type": 8, "data": {"choices": []}});
    }

    let searcher = state.reader.searcher();
    let query_parser = QueryParser::for_index(&state.index, state.default_fields.clone());
    let path_field = state.index.schema().get_field("path").unwrap();

    let mut choices: Vec<serde_json::Value> = Vec::new();

    if let Ok(query) = query_parser.parse_query(partial) {
        if let Ok(results) = searcher.search(&query, &TopDocs::with_limit(25)) {
            for (_, addr) in results {
                let Ok(doc) = searcher.doc::<TantivyDocument>(addr) else {
                    continue;
                };
                let Some(path) = doc.get_first(path_field).and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(parsed) = state.path_to_parsed.get(path) else {
                    continue;
                };
                let Some(title) = &parsed.title else {
                    continue;
                };
                choices.push(json!({"name": title, "value": title}));
            }
        }
    }

    json!({"type": 8, "data": {"choices": choices}})
}

fn build_embed_json(page: &FormattedPage, page_idx: usize) -> serde_json::Value {
    let page_idx = page_idx.min(page.pages.len().saturating_sub(1));
    let body = &page.pages[page_idx];

    let mut footer_text = page.footer.clone();
    if page.pages.len() > 1 {
        if !footer_text.is_empty() {
            footer_text.push_str(" · ");
        }
        footer_text.push_str(&format!("Page {}/{}", page_idx + 1, page.pages.len()));
    }

    let mut embed = json!({
        "title": page.title,
        "url": page.url,
        "color": EMBED_COLOR,
        "description": body,
        "footer": {"text": footer_text}
    });

    if page_idx == 0 && !page.fields.is_empty() {
        let fields: Vec<serde_json::Value> = page
            .fields
            .iter()
            .map(|(k, v)| json!({"name": k, "value": v, "inline": false}))
            .collect();
        embed["fields"] = json!(fields);
    }

    embed
}

fn build_components_json(
    page: &FormattedPage,
    page_idx: usize,
    data: &AppState,
) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();

    if page.pages.len() > 1 {
        let prev_id = format!("p:{}:{}", page.path, page_idx);
        let next_id = format!("n:{}:{}", page.path, page_idx);

        rows.push(json!({
            "type": 1,
            "components": [
                {
                    "type": 2,
                    "style": 2,
                    "emoji": {"name": "◀"},
                    "custom_id": prev_id,
                    "disabled": page_idx == 0
                },
                {
                    "type": 2,
                    "style": 2,
                    "emoji": {"name": "▶"},
                    "custom_id": next_id,
                    "disabled": page_idx >= page.pages.len() - 1
                }
            ]
        }));
    }

    if !page.see_also.is_empty() {
        let options: Vec<serde_json::Value> = page
            .see_also
            .iter()
            .filter(|link| get_page(&link.path.to_lowercase(), data).is_some())
            .map(|link| json!({"label": link.label, "value": link.path}))
            .collect();

        if !options.is_empty() {
            rows.push(json!({
                "type": 1,
                "components": [{
                    "type": 3,
                    "custom_id": "s",
                    "placeholder": "See also...",
                    "options": options
                }]
            }));
        }
    }

    rows
}

async fn register_commands(bot_token: &str, app_id: &str) {
    let client = reqwest::Client::new();
    let url = format!("https://discord.com/api/v10/applications/{app_id}/commands");
    let commands = json!([{
        "name": "dmref",
        "description": "Get an entry from the DM Reference",
        "type": 1,
        "integration_types": [0, 1],
        "contexts": [0, 1, 2],
        "options": [{
            "name": "search_for",
            "description": "The ref entry to look for",
            "type": 3,
            "required": true,
            "autocomplete": true
        }]
    }]);
    let resp = client
        .put(&url)
        .header("Authorization", format!("Bot {bot_token}"))
        .json(&commands)
        .send()
        .await
        .expect("failed to register commands");
    tracing::info!("registered commands: {}", resp.status());
}

fn clean_query(query: String) -> String {
    query.trim().to_lowercase()
}

fn get_page<'a>(query: &str, data: &'a AppState) -> Option<&'a str> {
    let path_find = query.replace(' ', "/");
    let path_find = path_find.strip_prefix('/').unwrap_or(&path_find);

    if let Some(key) = data
        .path_to_text
        .get_key_value(format!("{}/index.md", path_find).as_str())
    {
        return Some(*key.0);
    }

    if let Some(key) = data
        .path_to_text
        .get_key_value(format!("{}.md", path_find).as_str())
    {
        return Some(*key.0);
    }

    if path_find.contains('/') {
        let components: Vec<&str> = path_find.split('/').collect();

        let mut var = components.clone();
        var.insert(components.len() - 1, "var");
        let var_path = var.join("/");

        if let Some(key) = data
            .path_to_text
            .get_key_value(format!("{}.md", var_path).as_str())
        {
            return Some(*key.0);
        }

        let proc_path = var_path.replacen("/var/", "/proc/", 1);
        if let Some(key) = data
            .path_to_text
            .get_key_value(format!("{}.md", proc_path).as_str())
        {
            return Some(*key.0);
        }
    }

    if let Some(path) = data.titles_to_path.get(query) {
        return Some(*path);
    }

    let searcher = data.reader.searcher();
    let query_parser = QueryParser::for_index(&data.index, data.default_fields.clone());

    if let Ok(parsed) = query_parser.parse_query(query) {
        if let Ok(res) = searcher.search(&parsed, &TopDocs::with_limit(1)) {
            if let Some(doc_tuple) = res.first() {
                let doc: TantivyDocument = searcher.doc(doc_tuple.1).unwrap();
                for field in doc.iter_fields_and_values() {
                    if let Some(path) = data.path_to_text.get_key_value(field.1.as_str().unwrap()) {
                        return Some(*path.0);
                    }
                }
            }
        }
    }

    for key in data.path_to_text.keys() {
        if key.contains(path_find) {
            return Some(*key);
        }
    }

    None
}

fn extract_see_also(body: &str) -> Vec<SeeAlsoLink> {
    let see_also_regex = Regex::new(r"(?ms)^### See also\n(.*)$").unwrap();
    let link_regex = Regex::new(r"\[([^\]]+)]\(/([^)]+)\)").unwrap();

    let Some(capture) = see_also_regex.captures(body) else {
        return vec![];
    };

    let section = capture.get(1).unwrap().as_str();
    let mut seen = std::collections::HashSet::new();
    link_regex
        .captures_iter(section)
        .filter_map(|c| {
            let path = c.get(2).unwrap().as_str().to_string();
            if seen.insert(path.clone()) {
                Some(SeeAlsoLink {
                    label: c.get(1).unwrap().as_str().to_string(),
                    path,
                })
            } else {
                None
            }
        })
        .take(25)
        .collect()
}

fn format_page(page: &str, data: &AppState) -> Option<FormattedPage> {
    let body_regex = Regex::new(r"(?s)\+\+\+(.*)\+\+\+(.*)").unwrap();

    let parsed = data.path_to_parsed.get(page)?;
    let raw = data.path_to_text.get(page)?;

    let body = body_regex.captures(raw)?.get(2)?.as_str();

    let title = parsed.title.clone()?;
    let mut footer = String::new();

    let components: Vec<&str> = page.split('/').collect();

    let is_proc = components.contains(&"proc");
    let is_var = components.contains(&"var");

    if is_proc || is_var {
        if components.len() >= 3 {
            let mut parent_parts: Vec<&str> = components[..components.len() - 2].to_vec();
            parent_parts.push("index.md");
            let parent_path = parent_parts.join("/");

            if let Some(parent) = data.path_to_parsed.get(parent_path.as_str()) {
                if let Some(parent_title) = &parent.title {
                    footer = if is_proc {
                        format!("{} proc", parent_title)
                    } else {
                        format!("{} var", parent_title)
                    };
                }
            }
        }

        if footer.is_empty() && is_proc {
            footer = "global proc".to_string();
        }
    }

    let see_also = extract_see_also(body);
    let formatted_body = format_body(body);
    let url = get_url(page);

    let mut fields: Vec<(String, String)> = Vec::new();
    if let Some(headers) = &parsed.headers {
        let priority = ["Format", "Args", "Returns", "When", "Default action"];
        for key in &priority {
            if let Some(values) = headers.get(*key) {
                let field_value = values
                    .iter()
                    .map(|v| format!("`{}`", v))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !field_value.is_empty() {
                    fields.push((key.to_string(), truncate(&field_value, 1024)));
                }
            }
        }

        for (key, values) in headers {
            if priority.contains(&key.as_str()) {
                continue;
            }
            let field_value = values
                .iter()
                .map(|v| format!("`{}`", v))
                .collect::<Vec<_>>()
                .join("\n");
            if !field_value.is_empty() {
                fields.push((key.clone(), truncate(&field_value, 1024)));
            }
        }
    }

    let fields_len: usize = fields.iter().map(|(k, v)| k.len() + v.len()).sum();
    let overhead = title.len() + footer.len() + fields_len;
    let body_budget = MAX_EMBED_TOTAL.saturating_sub(overhead).min(MAX_DESCRIPTION);

    let pages = split_at_paragraphs(&formatted_body, body_budget);

    Some(FormattedPage {
        title,
        path: page.to_string(),
        url,
        footer,
        fields,
        pages,
        see_also,
    })
}

fn format_body(body: &str) -> String {
    let section_regex = Regex::new(r"(?m)^### .+\n((?:(?:>|-) .+\n?)*)").unwrap();
    let mut result = section_regex.replace_all(body, "").to_string();

    let see_also_regex = Regex::new(r"(?ms)^### See also\n.*$").unwrap();
    result = see_also_regex.replace_all(&result, "").to_string();

    let link_regex = Regex::new(r"\[([^\]]+)]\(/([^)]+)\)").unwrap();
    let mut new_result = result.clone();
    for capture in link_regex.captures_iter(&result) {
        let original = capture.get(0).unwrap().as_str();
        let display = capture.get(1).unwrap().as_str();
        let path = capture.get(2).unwrap().as_str();
        new_result = new_result.replace(
            original,
            &format!("[{}]({}/{})", display, BASE_URL, path),
        );
    }
    result = new_result;

    let code_fence_regex = Regex::new(r"```dream-maker[^\n]*").unwrap();
    result = code_fence_regex.replace_all(&result, "```js").to_string();

    let table_regex =
        Regex::new(r"(?m)^(\|[^\n]+\|\r?\n)((?:\| *:?[-]+:? *)+\|)(\n(?:\|[^\n]+\|\r?\n?)*)?$")
            .unwrap();
    let mut table_cleaned = result.clone();
    for capture in table_regex.captures_iter(&result) {
        let original = capture.get(0).unwrap().as_str();
        table_cleaned = table_cleaned.replace(original, &format!("```\n{}\n```", original));
    }
    result = table_cleaned;

    let whitespace_regex = Regex::new(r"\n{3,}").unwrap();
    result = whitespace_regex.replace_all(&result, "\n\n").to_string();

    result.trim().to_string()
}

fn get_url(page: &str) -> String {
    let mut path = page.replace(".md", "");
    path = path.replace("/index", "");
    format!("{}/{}", BASE_URL, path)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

fn split_at_paragraphs(text: &str, max_chunk: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    if text.len() <= max_chunk {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_chunk {
            chunks.push(remaining.to_string());
            break;
        }

        let search_area = &remaining[..max_chunk];

        let split_at = search_area
            .rfind("\n\n")
            .or_else(|| search_area.rfind('\n'))
            .unwrap_or(max_chunk);

        let split_at = if split_at == 0 { max_chunk } else { split_at };

        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start_matches('\n');
    }

    chunks
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let public_key_hex = std::env::var("DISCORD_PUBLIC_KEY").expect("missing DISCORD_PUBLIC_KEY");
    let bot_token = std::env::var("DISCORD_BOT_TOKEN").expect("missing DISCORD_BOT_TOKEN");
    let app_id = std::env::var("DISCORD_APPLICATION_ID").expect("missing DISCORD_APPLICATION_ID");
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());

    let public_key_bytes = hex::decode(&public_key_hex).expect("invalid public key hex");
    let public_key = VerifyingKey::from_bytes(
        &public_key_bytes
            .try_into()
            .expect("invalid public key length"),
    )
    .expect("invalid public key");

    let records = content::get_all();
    let mut path_to_parsed = HashMap::new();

    let search_index_path = TempDir::new().unwrap();

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("title", TEXT);
    schema_builder.add_text_field("path", TEXT | STORED);
    schema_builder.add_text_field("body", TEXT);

    let schema = schema_builder.build();
    let index = Index::create_in_dir(&search_index_path, schema.clone()).unwrap();
    let mut index_writer: IndexWriter = index.writer(15_000_000).unwrap();

    let titles =
        generate_titles_to_page(&records, &mut path_to_parsed, &schema, &mut index_writer)
            .unwrap();

    let reader = index
        .reader_builder()
        .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
        .try_into()
        .unwrap();

    let default_fields = vec![
        schema.get_field("title").unwrap(),
        schema.get_field("path").unwrap(),
        schema.get_field("body").unwrap(),
    ];

    register_commands(&bot_token, &app_id).await;

    let state = Arc::new(AppState {
        public_key,
        titles_to_path: titles,
        path_to_parsed,
        path_to_text: records,
        reader,
        index,
        default_fields,
    });

    let app = Router::new()
        .route("/", post(handle_interaction))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("listening on {port}");
    axum::serve(listener, app).await.unwrap();
}

fn generate_titles_to_page(
    records: &HashMap<&'static str, &'static str>,
    path_to_parsed: &mut HashMap<String, PageFrontmatter>,
    schema: &Schema,
    index_writer: &mut IndexWriter,
) -> Result<HashMap<String, &'static str>, Box<dyn std::error::Error>> {
    let mut title_map = HashMap::new();
    let frontmatter_regex = Regex::new(r"(?s)\+\+\+(.*)\+\+\+")?;

    let title_field = schema.get_field("title").unwrap();
    let path_field = schema.get_field("path").unwrap();
    let body_field = schema.get_field("body").unwrap();

    for record in records.iter() {
        let frontmatter = match frontmatter_regex.captures(record.1) {
            Some(front) => match front.get(1) {
                Some(capture) => capture.as_str(),
                None => continue,
            },
            None => continue,
        };

        let parsed: PageFrontmatter = match toml::from_str(frontmatter) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let title = parsed.title.clone().unwrap_or_default();
        let path = record.0.to_string();

        index_writer.add_document(doc!(
            title_field => title.clone(),
            path_field => path.clone(),
            body_field => record.1.to_string(),
        ))?;

        title_map.insert(title.to_lowercase(), *record.0);
        path_to_parsed.insert(path, parsed);
    }

    index_writer.commit()?;

    Ok(title_map)
}

#[derive(Deserialize, Clone)]
struct PageFrontmatter {
    title: Option<String>,
    #[allow(dead_code)]
    tags: Option<Vec<String>>,
    #[allow(dead_code)]
    byond_version: Option<String>,
    headers: Option<HashMap<String, Vec<String>>>,
}
