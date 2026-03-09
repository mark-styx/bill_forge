use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use diesel::prelude::*;
use fuzzy_matcher::skim::SkimMatcher;
use serde::{Deserialize, Serialize};

mod schema;
pub mod models;

// Database connection pool
#[derive(Clone)]
pub struct DbPool(diesel::pg::PgConnection);

impl From<diesel::pg::PgConnection> for DbPool {
    fn from(conn: diesel::pg::PgConnection) -> Self {
        DbPool(conn)
    }
}

// Define the fuzzy matcher
type Matcher = SkimMatcher;

#[derive(Serialize, Deserialize)]
pub struct InvoiceData {
    pub id: i32,
    pub invoice_number: String,
    pub vendor_name: String,
    pub amount: f64,
}

async fn create_vendor(pool: web::Data<DbPool>, new_vendor: web::Json<Vendor>) -> impl Responder {
    use schema::vendors::dsl::*;

    let conn = pool.0.get().unwrap();
    diesel::insert_into(vendors)
        .values(&new_vendor.into_inner())
        .get_result::<Vendor>(&conn)
        .map_err(|e| HttpResponse::InternalServerError().body(format!("Database error: {}", e)))
}

async fn get_vendors(pool: web::Data<DbPool>) -> impl Responder {
    use schema::vendors::dsl::*;

    let conn = pool.0.get().unwrap();
    vendors.load::<Vendor>(&conn)
        .map_err(|e| HttpResponse::InternalServerError().body(format!("Database error: {}", e)))
}

async fn match_invoice_data(pool: web::Data<DbPool>, invoice_data: web::Json<InvoiceData>) -> impl Responder {
    use schema::invoice_data::dsl::*;
    use schema::vendors::dsl::*;

    let conn = pool.0.get().unwrap();
    let matcher = Matcher::default();

    let invoice_data = invoice_data.into_inner();
    let vendor_name = &invoice_data.vendor_name;

    // Fuzzy match the vendor name
    let matched_vendors: Vec<Vendor> = vendors.filter(name.like(format!("%{}%", vendor_name))).load::<Vendor>(&conn).unwrap_or_else(|e| {
        error!("Database error: {}", e);
        vec![]
    });

    if !matched_vendors.is_empty() {
        // Find the best match
        let mut best_match: Option<Vendor> = None;
        let mut min_distance = f64::MAX;

        for vendor in matched_vendors {
            let distance = matcher.distance(&vendor.name, vendor_name);
            if distance < min_distance {
                min_distance = distance;
                best_match = Some(vendor);
            }
        }

        if let Some(best_vendor) = best_match {
            // Update the invoice_data with the best match
            diesel::update(invoice_data_table.find(invoice_data.id))
                .set((vendor_name.eq(&best_vendor.name), address.eq(&best_vendor.address)))
                .get_result::<InvoiceData>(&conn)
                .map_err(|e| HttpResponse::InternalServerError().body(format!("Database error: {}", e)))
        } else {
            Ok(HttpResponse::UnprocessableEntity().json("No matching vendor found"))
        }
    } else {
        Ok(HttpResponse::NotFound().json("Vendor not found"))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgConnection::establish(&database_url).unwrap();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(DbPool(pool.clone())))
            .service(web::resource("/vendors")
                .route(web::get().to(get_vendors))
                .route(web::post().to(create_vendor)))
            .service(web::resource("/invoice_data").route(web::post().to(match_invoice_data)))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}