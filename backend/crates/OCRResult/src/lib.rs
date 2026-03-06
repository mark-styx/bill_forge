use actix_web::{web, App, HttpResponse, HttpServer};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::env;

#[derive(Serialize, Deserialize, Debug)]
pub struct OCRResult {
    pub id: i32,
    pub document_id: String,
    pub result_text: Option<String>,
    pub status: OCRStatus,
    pub created_at: NaiveDateTime,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum OCRStatus {
    Pending,
    Success,
    Error(String),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Document {
    document_id: String,
}

async fn enqueue_ap_document(doc: web::Json<Document>, pool: web::Data<PgPoolOptions>) -> HttpResponse {
    let document_id = &doc.document_id;
    match insert_ocr_result(pool.clone(), document_id, OCRStatus::Pending).await {
        Ok(_) => HttpResponse::Ok().body("Document enqueued successfully"),
        Err(e) => {
            error!("Error inserting OCR result: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Failed to enqueue document: {}", e))
        }
    }
}

async fn enqueue_error_document(doc: web::Json<Document>, pool: web::Data<PgPoolOptions>) -> HttpResponse {
    let document_id = &doc.document_id;
    match insert_ocr_result(pool.clone(), document_id, OCRStatus::Pending).await {
        Ok(_) => HttpResponse::Ok().body("Document enqueued successfully"),
        Err(e) => {
            error!("Error inserting OCR result: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Failed to enqueue document: {}", e))
        }
    }
}

async fn get_document_status(
    doc_id: web::Path<String>,
    pool: web::Data<PgPoolOptions>,
) -> HttpResponse {
    match get_ocr_result(pool.clone(), &doc_id).await {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => {
            error!("Error getting OCR result: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Failed to retrieve document status: {}", e))
        }
    }
}

async fn insert_ocr_result(pool: PgPoolOptions, document_id: &str, status: OCRStatus) -> Result<(), sqlx::Error> {
    let now = chrono::Local::now().naive_utc();
    sqlx::query!(
        "INSERT INTO ocr_results (document_id, result_text, status, created_at)
         VALUES ($1, $2, $3, $4)",
        document_id,
        None::<&str>,
        status.to_string(),
        now
    )
    .execute(&pool).await?;
    Ok(())
}

async fn get_ocr_result(pool: PgPoolOptions, document_id: &str) -> Result<OCRResult, sqlx::Error> {
    let result = sqlx::query_as!(
        OCRResult,
        "SELECT * FROM ocr_results WHERE document_id = $1",
        document_id
    )
    .fetch_one(&pool).await?;
    Ok(result)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .route("/ocr/queue/ap", web::post().to(enqueue_ap_document))
            .route("/ocr/queue/error", web::post().to(enqueue_error_document))
            .route("/ocr/queue/status/{document_id}", web::get().to(get_document_status))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}