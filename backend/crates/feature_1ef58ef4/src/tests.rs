#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Error as ReqwestError;
    use rusqlite::{Connection, Result as RusqliteResult};
    use std::io::Write;

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

    #[tokio::test]
    async fn test_create_invoice() {
        let db = setup_db();
        let invoice = Invoice {
            id: 1,
            image_url: "http://example.com/image.jpg".to_string(),
            ocr_provider: OCRProvider::TesseractLocal,
            result: None,
            created_at: NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
            updated_at: NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
        };

        create_invoice(Json(invoice.clone()), db.clone()).await.unwrap();

        let mut stmt = db.prepare("SELECT * FROM invoices WHERE id = ?1").unwrap();
        let result: Invoice = stmt.query_row(params![invoice.id], |row| {
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

        assert_eq!(result, invoice);
    }

    #[tokio::test]
    async fn test_select_ocr_provider() {
        let db = setup_db();
        let invoice = Invoice {
            id: 1,
            image_url: "http://example.com/image.jpg".to_string(),
            ocr_provider: OCRProvider::TesseractLocal,
            result: None,
            created_at: NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
            updated_at: NaiveDateTime::from_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64, 0),
        };

        create_invoice(Json(invoice.clone()), db.clone()).await.unwrap();

        select_ocr_provider(axum::extract::Path(1), Json(OCRProvider::AWSTextract), db.clone()).await.unwrap();

        let mut stmt = db.prepare("SELECT * FROM invoices WHERE id = ?1").unwrap();
        let result: Invoice = stmt.query_row(params![invoice.id], |row| {
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

        assert_eq!(result.ocr_provider, OCRProvider::AWSTextract);
    }
}