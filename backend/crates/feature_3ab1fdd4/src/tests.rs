#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::StatusCode, test};
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn setup_db() -> PgPoolOptions {
        let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
        PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .unwrap()
    }

    #[actix_web::test]
    async fn test_enqueue_ap_document() {
        let pool = setup_db().await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .route("/ocr/queue/ap", web::post().to(enqueue_ap_document)),
        )
        .await;

        let document_id = "12345";
        let req_body = format!("{{\"document_id\": \"{}\"}}", document_id);

        let resp = test::TestRequest::post()
            .uri("/ocr/queue/ap")
            .set_payload(req_body)
            .send_request(&app)
            .await;

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn test_get_document_status() {
        let pool = setup_db().await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .route("/ocr/queue/status/{document_id}", web::get().to(get_document_status)),
        )
        .await;

        let document_id = "12345";
        insert_ocr_result(pool.clone(), document_id, OCRStatus::Pending).await.unwrap();

        let resp = test::TestRequest::get()
            .uri(&format!("/ocr/queue/status/{}", document_id))
            .send_request(&app)
            .await;

        assert_eq!(resp.status(), StatusCode::OK);
    }
}