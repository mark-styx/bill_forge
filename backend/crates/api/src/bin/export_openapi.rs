//! Binary to export the OpenAPI specification as pretty-printed JSON.
//!
//! Usage: `cargo run -p billforge-api --bin export-openapi -- [output-path]`
//! Default output path: `packages/shared-types/openapi.json` (relative to repo root).

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let default_path = "../../packages/shared-types/openapi.json";
    let output_path = env::args()
        .nth(1)
        .unwrap_or_else(|| default_path.to_owned());

    let openapi = billforge_api::openapi::openapi_doc();
    let json =
        serde_json::to_string_pretty(&openapi).expect("failed to serialize OpenAPI doc to JSON");

    let path = Path::new(&output_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create output directory");
    }
    fs::write(path, json.as_bytes()).expect("failed to write OpenAPI JSON file");

    println!("OpenAPI spec written to {}", output_path);
}
