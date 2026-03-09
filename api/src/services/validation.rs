use crate::error::AppError;

pub fn validate_username(username: &str) -> Result<(), AppError> {
    if username.is_empty()
        || username.chars().any(char::is_whitespace)
        || username.chars().any(char::is_control)
    {
        return Err(AppError::BadRequest(
            "Invalid username: must not be empty or contain whitespace/control characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.is_empty()
        || password.len() < 8
        || password.len() > 256
        || password.chars().any(char::is_control)
    {
        return Err(AppError::BadRequest(
            "Invalid password: must be 8-256 characters and not contain control characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_username_empty() {
        assert!(validate_username("").is_err());
    }

    #[test]
    fn validate_username_whitespace() {
        assert!(validate_username("hello world").is_err());
    }

    #[test]
    fn validate_username_tab() {
        assert!(validate_username("hello\tworld").is_err());
    }

    #[test]
    fn validate_username_control_chars() {
        assert!(validate_username("hello\x00world").is_err());
    }

    #[test]
    fn validate_username_valid() {
        assert!(validate_username("admin").is_ok());
    }

    #[test]
    fn validate_username_valid_with_special_chars() {
        assert!(validate_username("admin-user_1").is_ok());
    }

    #[test]
    fn validate_password_empty() {
        assert!(validate_password("").is_err());
    }

    #[test]
    fn validate_password_too_short() {
        assert!(validate_password("short").is_err());
    }

    #[test]
    fn validate_password_seven_chars() {
        assert!(validate_password("1234567").is_err());
    }

    #[test]
    fn validate_password_control_chars() {
        assert!(validate_password("password\x01long_enough").is_err());
    }

    #[test]
    fn validate_password_valid() {
        assert!(validate_password("password123").is_ok());
    }

    #[test]
    fn validate_password_exactly_8_chars() {
        assert!(validate_password("12345678").is_ok());
    }

    #[test]
    fn validate_password_exactly_256_chars() {
        let pw = "a".repeat(256);
        assert!(validate_password(&pw).is_ok());
    }

    #[test]
    fn validate_password_257_chars_rejected() {
        let pw = "a".repeat(257);
        assert!(validate_password(&pw).is_err());
    }
}
