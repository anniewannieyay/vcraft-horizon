//! Small helpers shared by the rest of the engine.

use serde_json::Value;

pub fn parse_uuid(s: &str) -> Option<u128> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    (hex.len() == 32).then(|| u128::from_str_radix(&hex, 16).ok()).flatten()
}

pub fn uuid_string(u: u128) -> String {
    let h = format!("{u:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// A string field, `None` when missing or null (gson's `str`). Numbers and
/// booleans read as their text, as `getAsString` does.
pub fn s(o: &Value, k: &str) -> Option<String> {
    match o.get(k)? {
        Value::Null => None,
        Value::String(x) => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn b(o: &Value, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

pub fn i(o: &Value, k: &str) -> i64 {
    o.get(k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(0)
}

/// Java's `String.hashCode`.
pub fn java_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Drops control characters and section signs, trims, at most `max` characters (`NowPlaying.clean`).
pub fn clean_song(s: Option<&str>, max: usize) -> Option<String> {
    let out: String = s?.chars().filter(|c| !c.is_control() && *c != '\u{a7}').collect();
    let mut out = out.trim().to_owned();
    if out.chars().count() > max {
        out = out.chars().take(max).collect::<String>().trim().to_owned();
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hashes_like_java() {
        assert_eq!(super::java_hash("hello"), 99162322);
        assert_eq!(super::java_hash(""), 0);
    }
}
