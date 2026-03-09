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
        || password.chars().any(char::is_control)
    {
        return Err(AppError::BadRequest(
            "Invalid password: must be at least 8 characters and not contain control characters"
                .into(),
        ));
    }
    Ok(())
}
