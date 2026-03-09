use ocr_engine::process_invoice;
use database::{store_data, retrieve_data};

#[test]
fn test_component_interaction() {
    let sample_image_path = "path/to/sample_invoice.png";
    let image_data = fs::read(sample_image_path).unwrap();
    
    match process_invoice(&image_data) {
        Ok(data) => store_data(data),
        Err(_) => panic!("System should handle errors and still attempt to store data"),
    }
    
    let retrieved_data = retrieve_data().unwrap();
    assert!(retrieved_data.len() > 0);
}