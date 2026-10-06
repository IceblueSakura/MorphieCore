//! Operator image budgets apply after typed decode and before any projection/publication.
use super::*;
use crate::semantic::task::image_generation::{
    GeneratedImage, ImageData, ImageGenerationRequest, ImageGenerationResponse,
};
use base64::Engine;
#[test]
fn image_decoded_budget_is_independent_and_inclusive() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; 3 << 20]);
    let response = ImageGenerationResponse::new(
        1,
        vec![GeneratedImage::new(ImageData::new(&encoded).unwrap())],
    )
    .unwrap();
    let request = ImageGenerationRequest::new("synthetic").unwrap();
    assert!(
        check_response(
            &response,
            &request,
            super::super::Limits::default().image_bytes,
            super::super::Limits::default().images_bytes
        )
        .is_err()
    );
    assert!(check_response(&response, &request, 3 << 20, 3 << 20).is_ok());
    assert!(check_response(&response, &request, (3 << 20) - 1, 3 << 20).is_err());
}

#[test]
fn operator_collection_and_per_image_limits_are_independent() {
    use crate::semantic::value::Presence;
    let response = ImageGenerationResponse::new(
        1,
        vec![
            GeneratedImage::new(ImageData::new("AQID").unwrap()),
            GeneratedImage::new(ImageData::new("BAUG").unwrap()),
        ],
    )
    .unwrap();
    let mut request = ImageGenerationRequest::new("two").unwrap();
    request.count = Presence::Value(2);
    assert!(check_response(&response, &request, 3, 6).is_ok());
    assert!(check_response(&response, &request, 2, 6).is_err());
    assert!(check_response(&response, &request, 3, 5).is_err());
}
