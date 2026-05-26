use std::time::{SystemTime, UNIX_EPOCH};

pub fn timestamp_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub fn new_comment_id() -> String {
    format!("c-{}", timestamp_now())
}
