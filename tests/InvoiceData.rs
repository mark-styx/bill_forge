use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct InvoiceData {
    description: String,
    quantity: i32,
    unit_price: f64,
    tax: f64,
}

#[derive(Serialize, Deserialize)]
struct LineItem {
    description: String,
    quantity: i32,
    unit_price: f64,
    tax: f64,
}

fn validate_invoice_data(data: &InvoiceData) -> bool {
    data.quantity >= 0 && data.unit_price >= 0.0 && data.tax >= 0.0
}

async fn upload_invoice_image(image_path: &str) -> Result<http::Response, reqwest::Error> {
    let body = std::fs::read(image_path)?;
    let client = reqwest::Client::new();
    client.post("http://localhost:3000/upload")
        .body(body)
        .send()
        .await
}