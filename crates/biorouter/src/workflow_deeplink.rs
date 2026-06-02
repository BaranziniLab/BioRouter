use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use thiserror::Error;

use crate::workflow::Workflow;

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Failed to decode workflow deeplink")]
    AllMethodsFailed,
}

pub fn encode(workflow: &Workflow) -> Result<String, serde_json::Error> {
    let workflow_json = serde_json::to_string(workflow)?;
    let encoded = URL_SAFE_NO_PAD.encode(workflow_json.as_bytes());
    Ok(encoded)
}

pub fn decode(link: &str) -> Result<Workflow, DecodeError> {
    // Handle the current format: URL-safe Base64 without padding.
    if let Ok(decoded_bytes) = URL_SAFE_NO_PAD.decode(link) {
        if let Ok(workflow_json) = String::from_utf8(decoded_bytes) {
            if let Ok(workflow) = serde_json::from_str::<Workflow>(&workflow_json) {
                return Ok(workflow);
            }
        }
    }

    // Handle legacy formats of 'standard base64 encoded' and standard base64 encoded that was then url encoded.
    if let Ok(url_decoded) = urlencoding::decode(link) {
        if let Ok(decoded_bytes) =
            base64::engine::general_purpose::STANDARD.decode(url_decoded.as_bytes())
        {
            if let Ok(workflow_json) = String::from_utf8(decoded_bytes) {
                if let Ok(workflow) = serde_json::from_str::<Workflow>(&workflow_json) {
                    return Ok(workflow);
                }
            }
        }
    }

    Err(DecodeError::AllMethodsFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Workflow;

    fn create_test_workflow() -> Workflow {
        Workflow::builder()
            .title("Test Workflow")
            .description("A test workflow for deeplink encoding/decoding")
            .instructions("Act as a helpful assistant")
            .build()
            .expect("Failed to build test workflow")
    }

    #[test]
    fn test_encode_decode_round_trip() {
        let original_workflow = create_test_workflow();

        let encoded = encode(&original_workflow).expect("Failed to encode workflow");
        assert!(!encoded.is_empty());

        let decoded_workflow = decode(&encoded).expect("Failed to decode workflow");

        assert_eq!(original_workflow.title, decoded_workflow.title);
        assert_eq!(original_workflow.description, decoded_workflow.description);
        assert_eq!(
            original_workflow.instructions,
            decoded_workflow.instructions
        );
        assert_eq!(original_workflow.version, decoded_workflow.version);
    }

    #[test]
    fn test_decode_legacy_standard_base64() {
        let workflow = create_test_workflow();
        let workflow_json = serde_json::to_string(&workflow).unwrap();
        let legacy_encoded =
            base64::engine::general_purpose::STANDARD.encode(workflow_json.as_bytes());

        let decoded_workflow = decode(&legacy_encoded).expect("Failed to decode legacy format");
        assert_eq!(workflow.title, decoded_workflow.title);
        assert_eq!(workflow.description, decoded_workflow.description);
        assert_eq!(workflow.instructions, decoded_workflow.instructions);
    }

    #[test]
    fn test_decode_legacy_url_encoded_base64() {
        let workflow = create_test_workflow();
        let workflow_json = serde_json::to_string(&workflow).unwrap();
        let base64_encoded =
            base64::engine::general_purpose::STANDARD.encode(workflow_json.as_bytes());
        let url_encoded = urlencoding::encode(&base64_encoded);

        let decoded_workflow =
            decode(&url_encoded).expect("Failed to decode URL-encoded legacy format");
        assert_eq!(workflow.title, decoded_workflow.title);
        assert_eq!(workflow.description, decoded_workflow.description);
        assert_eq!(workflow.instructions, decoded_workflow.instructions);
    }

    #[test]
    fn test_decode_invalid_input() {
        let result = decode("invalid_base64!");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DecodeError::AllMethodsFailed));
    }
}
