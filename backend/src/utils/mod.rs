// utils/mod.rs
pub mod error;
pub mod config;
pub mod security;
pub mod validation;
pub mod helpers;

// Ré-exports pour faciliter l'import
pub use error::{AppError, Result};
pub use config::Config;
pub use security::{
    generate_access_token, generate_refresh_token,
    verify_access_token, verify_refresh_token,
    hash_password, verify_password,
    generate_api_key, generate_reset_token,
    encrypt_data, decrypt_data, sha256_hash,
    validate_password_strength,
};
pub use validation::{
    validate_email, validate_password, validate_filename,
    validate_file_size, validate_model_format,
    validate_quantization_method, validate_plan,
    validate_uuid, validate_url, validate_file_path,
    validate_positive_number, validate_percentage,
    validate_non_empty_string, validate_non_empty_list,
    validate_object,
};
pub use helpers::{
    generate_uuid, format_date, format_relative_date,
    format_file_size, calculate_percentage,
    truncate_string, sanitize_filename,
    ensure_directory_exists, remove_directory,
    read_file_bytes, write_file_bytes, get_file_size,
    is_file, is_directory, get_file_extension,
    generate_unique_filename, format_duration,
    generate_csrf_token, validate_csrf_token,
    delay_ms, with_timeout,
};