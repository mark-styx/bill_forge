#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use serde_json::json;

    #[actix_web::test]
    async fn test_get_invoices() {
        let mut client = Client::connect("postgres://username:password@localhost/dbname").unwrap();
        client.execute(
            "INSERT INTO invoices (vendor_name, amount, date, status) VALUES ('Test Vendor', 100.00, '2023-04-01 00:00:00', 'Pending')",
            &[],
        ).unwrap();

        let app = test::init_service(App::new()
            .data(client)
            .route("/invoices", web::get().to(get_invoices))
        ).await;

        let req = test::TestRequest::get()
            .uri("/invoices")
            .send_request(&app)
            .await;

        assert!(req.status().is_success());
        let body = test::read_body(req).await;
        println!("Body: {:?}", String::from_utf8(body.clone()).unwrap());

        let invoices: Vec<Invoice> = serde_json::from_slice(&body).unwrap();
        assert_eq!(invoices.len(), 1);
        assert_eq!(invoices[0].vendor_name, "Test Vendor");
    }

    #[actix_web::test]
    async fn test_create_invoice() {
        let client = Client::connect("postgres://username:password@localhost/dbname").unwrap();

        let app = test::init_service(App::new()
            .data(client)
            .route("/invoices", web::post().to(create_invoice))
        ).await;

        let req = test::TestRequest::post()
            .uri("/invoices")
            .set_payload(json!({
                "vendor_name": "New Vendor",
                "amount": 200.00,
                "date": "2023-04-01 00:00:00",
                "status": "Pending"
            }).to_string())
            .send_request(&app)
            .await;

        assert!(req.status().is_success());
    }

    #[actix_web::test]
    async fn test_update_invoice() {
        let mut client = Client::connect("postgres://username:password@localhost/dbname").unwrap();
        client.execute(
            "INSERT INTO invoices (vendor_name, amount, date, status) VALUES ('Test Vendor', 100.00, '2023-04-01 00:00:00', 'Pending')",
            &[],
        ).unwrap();

        let app = test::init_service(App::new()
            .data(client)
            .route("/invoices/{id}", web::put().to(update_invoice))
        ).await;

        let req = test::TestRequest::put()
            .uri("/invoices/1")
            .set_payload(json!({
                "vendor_name": "Updated Vendor",
                "amount": 300.00,
                "date": "2023-04-01 00:00:00",
                "status": "Approved"
            }).to_string())
            .send_request(&app)
            .await;

        assert!(req.status().is_success());
    }

    #[actix_web::test]
    async fn test_delete_invoice() {
        let mut client = Client::connect("postgres://username:password@localhost/dbname").unwrap();
        client.execute(
            "INSERT INTO invoices (vendor_name, amount, date, status) VALUES ('Test Vendor', 100.00, '2023-04-01 00:00:00', 'Pending')",
            &[],
        ).unwrap();

        let app = test::init_service(App::new()
            .data(client)
            .route("/invoices/{id}", web::delete().to(delete_invoice))
        ).await;

        let req = test::TestRequest::delete()
            .uri("/invoices/1")
            .send_request(&app)
            .await;

        assert!(req.status().is_success());
    }
}