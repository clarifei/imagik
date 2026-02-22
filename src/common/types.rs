/// Common error type used for request-path operations.
pub type AppError = String;

/// Standard result alias used across storage/cache/handler boundaries.
pub type AppResult<T> = Result<T, AppError>;
