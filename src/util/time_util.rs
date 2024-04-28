use std::time::{SystemTime, UNIX_EPOCH};

pub fn epoch_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
