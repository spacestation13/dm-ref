use std::{
    error::Error,
    fs::{self, File},
    io::Write,
    path::Path,
};

const SOURCE_DIR: &str = "../content";
// The site build's search index (`npx quartz build` writes public/static/search)
const SEARCH_DIR: &str = "../public/static/search";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=SOURCE_DIR");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("content.rs");
    let from_path = std::env::var("SOURCE_DIR").unwrap_or(SOURCE_DIR.to_string());
    let from_path_resolved = Path::new(&from_path).canonicalize().unwrap_or_else(|e| {
        panic!("Cannot resolve SOURCE_DIR '{}': {}", from_path, e);
    });
    let from_path = from_path_resolved.to_str().unwrap().to_string();

    let mut out_files = File::create(&dest_path)?;

    writeln!(
        &mut out_files,
        r##"use std::collections::HashMap;pub fn get_all() -> HashMap<&'static str, &'static str> {{ let mut out = HashMap::new();"##,
    )?;

    visit_dir(&mut out_files, from_path.as_str(), &from_path)?;

    writeln!(&mut out_files, r##"out}}"##)?;

    embed_search_shards(Path::new(&out_dir).join("search_shards.rs").as_path())?;

    Ok(())
}

// Writes `SHARDS`, every search shard file as a string. The word list is derived from the shards.
fn embed_search_shards(dest_path: &Path) -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=SEARCH_DIR");
    let search_dir = std::env::var("SEARCH_DIR").unwrap_or(SEARCH_DIR.to_string());
    let search_dir = Path::new(&search_dir).canonicalize().unwrap_or_else(|e| {
        panic!("Cannot resolve SEARCH_DIR '{search_dir}' (build the site first): {e}");
    });

    let mut shards = Vec::new();
    for entry in fs::read_dir(&search_dir)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name != "words.json") {
            shards.push(path);
        }
    }
    shards.sort();

    let mut out_file = File::create(dest_path)?;
    writeln!(out_file, "pub static SHARDS: &[&str] = &[")?;
    for shard in shards {
        writeln!(out_file, r##"include_str!(r#"{}"#),"##, shard.display())?;
    }
    writeln!(out_file, "];")?;
    Ok(())
}

fn visit_dir(file: &mut File, dir: &str, from_dir: &str) -> Result<(), Box<dyn Error>> {
    for inner_file in fs::read_dir(dir)? {
        let inner_file = inner_file?;
        let file_type = inner_file.file_type()?;

        if !file_type.is_file() {
            if file_type.is_dir() {
                visit_dir(file, inner_file.path().to_str().unwrap(), from_dir)?;
            }
            continue;
        }

        let path = inner_file.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        writeln!(
            file,
            r##"out.insert("{name}", include_str!(r#"{path}"#));"##,
            name = path
                .strip_prefix(from_dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            path = path.canonicalize().unwrap().display(),
        )?;
    }

    Ok(())
}
