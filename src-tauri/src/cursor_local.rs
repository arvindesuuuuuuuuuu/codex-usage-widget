use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{types::ValueRef, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::{path::Path, time::Duration};

const INVALID: &str =
    "Cursor's saved sign-in could not be read. Open Cursor and sign in again, or use Web sign-in.";

// Reads only the access-token row. Tokens never leave the native backend except
// for the authenticated HTTPS request to Cursor; no refresh tokens are accessed.
pub(super) fn read_cookie(path: &Path, now: u64) -> Result<Option<String>, String> {
    match path.try_exists() {
        Ok(false) => return Ok(None),
        Ok(true) => {}
        Err(_) => return Err("Cursor's local sign-in storage is not accessible.".into()),
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "Could not open Cursor's local sign-in storage. Try again after Cursor finishes starting.".to_string())?;
    connection
        .busy_timeout(Duration::from_millis(500))
        .map_err(|_| INVALID.to_string())?;
    let token = read_token(&connection)?;
    token
        .filter(|token| !token.is_empty())
        .map(|token| token_cookie(&token, now))
        .transpose()
}

fn read_token(connection: &Connection) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1",
            ["cursorAuth/accessToken"],
            |row| match row.get_ref(0)? {
                ValueRef::Text(bytes) | ValueRef::Blob(bytes) => Ok(decode_token(bytes)),
                ValueRef::Null => Ok(Ok(String::new())),
                _ => Ok(Err(INVALID.to_string())),
            },
        )
        .optional()
        .map_err(|_| {
            "Could not read Cursor's local sign-in. Its storage format may have changed."
                .to_string()
        })?
        .transpose()
}

fn decode_token(bytes: &[u8]) -> Result<String, String> {
    let (bytes, utf16) = if bytes.starts_with(&[0xff, 0xfe]) {
        (&bytes[2..], true)
    } else {
        (
            bytes,
            !bytes.is_empty()
                && bytes.len() % 2 == 0
                && bytes
                    .chunks_exact(2)
                    .all(|pair| pair[0].is_ascii() && pair[1] == 0),
        )
    };
    if utf16 {
        if bytes.len() % 2 != 0 {
            return Err(INVALID.into());
        }
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| INVALID.into())
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| INVALID.into())
    }
}

fn token_cookie(token: &str, now: u64) -> Result<String, String> {
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        return Err(INVALID.into());
    }
    let payload = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| INVALID.to_string())?;
    let claims: Value = serde_json::from_slice(&payload).map_err(|_| INVALID.to_string())?;
    // This is expiry/shape checking only. Cursor verifies the token's signature.
    let expiry = claims.get("exp").and_then(Value::as_u64).ok_or(INVALID)?;
    if expiry <= now.saturating_add(60) {
        return Err("Cursor's saved sign-in has expired. Open Cursor to renew it, then refresh the widget; or use Web sign-in.".into());
    }
    let user = claims
        .get("sub")
        .and_then(Value::as_str)
        .and_then(|subject| subject.rsplit('|').next())
        .filter(|user| !user.is_empty())
        .ok_or(INVALID)?;
    if !user
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(INVALID.into());
    }
    Ok(format!("WorkosCursorSessionToken={user}%3A%3A{token}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token(subject: &str, expiry: u64) -> String {
        format!(
            "e30.{}.test",
            URL_SAFE_NO_PAD.encode(json!({"sub":subject,"exp":expiry}).to_string())
        )
    }

    #[test]
    fn rejects_expired_malformed_and_header_injection_tokens() {
        for value in [
            token("user_1", 100),
            token("user_1", 160),
            token("user;bad", 999),
            "not-a-jwt".into(),
            format!("{}\r\nInjected: value", token("user_1", 999)),
        ] {
            let error = token_cookie(&value, 100).unwrap_err();
            assert!(!error.contains(&value));
        }
        let valid = token("auth0|user_1", 999);
        assert_eq!(
            token_cookie(&valid, 100).unwrap(),
            format!("WorkosCursorSessionToken=user_1%3A%3A{valid}")
        );
    }

    #[test]
    fn reads_utf8_and_utf16_blobs() {
        let value = token("user_1", 999);
        assert_eq!(decode_token(value.as_bytes()).unwrap(), value);
        let utf16: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_token(&utf16).unwrap(), value);
        let with_bom = [vec![0xff, 0xfe], utf16].concat();
        assert_eq!(decode_token(&with_bom).unwrap(), value);
    }

    #[test]
    fn reads_live_wal_and_account_changes_without_modifying_cursor_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.vscdb");
        assert!(read_cookie(&path, 100).unwrap().is_none());
        assert!(!path.exists());
        let writer = Connection::open(&path).unwrap();
        writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB);").unwrap();
        let first = token("user_first", 999);
        writer
            .execute(
                "INSERT INTO ItemTable VALUES(?1,?2)",
                ("cursorAuth/accessToken", first),
            )
            .unwrap();
        assert!(read_cookie(&path, 100)
            .unwrap()
            .unwrap()
            .contains("user_first%3A%3A"));
        let second = token("user_second", 999);
        let blob: Vec<u8> = second.encode_utf16().flat_map(u16::to_le_bytes).collect();
        writer
            .execute("UPDATE ItemTable SET value=?1", [&blob])
            .unwrap();
        assert!(read_cookie(&path, 100)
            .unwrap()
            .unwrap()
            .contains("user_second%3A%3A"));
        assert_eq!(
            writer
                .query_row("SELECT value FROM ItemTable", [], |row| row
                    .get::<_, Vec<u8>>(0))
                .unwrap(),
            blob
        );
        writer.execute("DELETE FROM ItemTable", []).unwrap();
        assert!(read_cookie(&path, 100).unwrap().is_none());
    }
}
