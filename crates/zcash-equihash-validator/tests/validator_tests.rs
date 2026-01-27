use zcash_equihash_validator::{EquihashValidator, ValidationError};

#[test]
fn test_validator_creation() {
    let validator = EquihashValidator::new();
    assert_eq!(validator.n(), 200);
    assert_eq!(validator.k(), 9);
}

#[test]
fn test_invalid_solution_rejected() {
    let validator = EquihashValidator::new();

    // All-zero header and solution should fail verification
    let header = [0u8; 140];
    let solution = [0u8; 1344];

    let result = validator.verify_solution(&header, &solution);
    assert!(result.is_err());
}

#[test]
fn test_wrong_solution_length_rejected() {
    let validator = EquihashValidator::new();

    let header = [0u8; 140];
    let bad_solution = [0u8; 100]; // Wrong length

    let result = validator.verify_solution(&header, &bad_solution);
    assert!(matches!(result, Err(ValidationError::InvalidSolutionLength(_))));
}
