use axum::{
    extract::{Json, Path},
    routing::{get, post},
    Router,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Invoice {
    id: i32,
    image_url: String,
    ocr_provider: OCRProvider,
    result: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
enum OCRProvider {
    TesseractLocal,
    AWSTextract,
    GoogleVision,
}

async fn create_invoice(Json(invoice): Json<Invoice>, db: Connection) -> String {
    let mut stmt = db.prepare("INSERT INTO invoices (image_url, ocr_provider, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)").unwrap();
    let _result = stmt.execute(params![
        invoice.image_url,
        match invoice.ocr_provider {
            OCRProvider::TesseractLocal => 0,
            OCRProvider::AWSTextract => 1,
            OCRProvider::GoogleVision => 2,
        },
        NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
        NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
    ]).unwrap();

    format!("Invoice created with ID: {}", invoice.id)
}

async fn get_all_invoices(db: Connection) -> String {
    let mut stmt = db.prepare("SELECT * FROM invoices").unwrap();
    let results = stmt.query_map([], |row| {
        Ok(Invoice {
            id: row.get(0)?,
            image_url: row.get(1)?,
            ocr_provider: match row.get::<_, i32>(2)? {
                0 => OCRProvider::TesseractLocal,
                1 => OCRProvider::AWSTextract,
                2 => OCRProvider::GoogleVision,
                _ => panic!("Invalid OCR provider"),
            },
            result: row.get(3),
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }).unwrap();

    let mut output = String::new();
    for result in results {
        write!(&mut output, "{}\n", serde_json::to_string(&result.unwrap()).unwrap()).unwrap();
    }

    output
}

async fn select_ocr_provider(
    Path(id): Path<i32>,
    Json(ocr_provider): Json<OCRProvider>,
    db: Connection,
) -> String {
    let mut stmt = db.prepare("UPDATE invoices SET ocr_provider = ?1, updated_at = ?2 WHERE id = ?3").unwrap();
    let _result = stmt.execute(params![
        match ocr_provider {
            OCRProvider::TesseractLocal => 0,
            OCRProvider::AWSTextract => 1,
            OCRProvider::GoogleVision => 2,
        },
        NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
        id,
    ]).unwrap();

    format!("OCR provider updated for invoice ID: {}", id)
}

#[tokio::main]
async fn main() {
    let db = setup_db();
    db.execute(
        "CREATE TABLE IF NOT EXISTS invoices (
            id INTEGER PRIMARY KEY,
            image_url TEXT NOT NULL,
            ocr_provider INTEGER NOT NULL,
            result TEXT,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        )",
        [],
    ).unwrap();

    let app = Router::new()
        .route("/invoices", post(create_invoice).get(get_all_invoices))
        .route("/invoices/:id/ocr_providers", post(select_ocr_provider));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn setup_db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS invoices (
            id INTEGER PRIMARY KEY,
            image_url TEXT NOT NULL,
            ocr_provider INTEGER NOT NULL,
            result TEXT,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        )",
        [],
    ).unwrap();
    db
}