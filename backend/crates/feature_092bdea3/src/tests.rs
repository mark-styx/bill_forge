#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::{embed_migrations, run_pending_migrations};
    use std::env;

    embed_migrations!("migrations");

    fn establish_connection() -> PgConnection {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        PgConnection::establish(&database_url).unwrap()
    }

    #[actix_web::test]
    async fn test_create_vendor() {
        run_pending_migrations!(&establish_connection());

        let pool = establish_connection().into();
        let vendor = web::Json(Vendor {
            id: 1,
            name: "Test Vendor".to_string(),
            address: "123 Test St".to_string(),
        });

        let response = create_vendor(web::Data::new(pool), vendor).await;
        assert_eq!(response.status(), 201);
    }

    #[actix_web::test]
    async fn test_get_vendors() {
        run_pending_migrations!(&establish_connection());

        let pool = establish_connection().into();
        let response = get_vendors(web::Data::new(pool)).await;
        assert_eq!(response.status(), 200);
    }

    #[actix_web::test]
    async fn test_match_invoice_data() {
        run_pending_migrations!(&establish_connection());

        let pool = establish_connection().into();
        let invoice_data = web::Json(InvoiceData {
            id: 1,
            invoice_number: "INV-001".to_string(),
            vendor_name: "Test Vendor".to_string(),
            amount: 100.0,
        });

        let response = match_invoice_data(web::Data::new(pool), invoice_data).await;
        assert_eq!(response.status(), 200);
    }
}