use std::{
    error::Error,
    fs::{self, File},
    io::Write,
    path::Path,
};

const SOURCE_DIR: &str = "../content";

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
