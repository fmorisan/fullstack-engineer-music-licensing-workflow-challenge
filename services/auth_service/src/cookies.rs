//! Cookie header plumbing for the refresh token (ADR-012).

use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;

use crate::refresh::COOKIE_NAME;

/// Extract the refresh token from a request's `Cookie` header.
#[must_use]
pub fn refresh_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())?;
    cookie.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == COOKIE_NAME && !value.is_empty()).then_some(value.to_string())
    })
}

/// Build the `Set-Cookie` value that stores a fresh refresh token.
#[must_use]
pub fn set_cookie(value: &str, max_age_secs: i64, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={value}; Path=/auth; Max-Age={max_age_secs}; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Build the `Set-Cookie` value that clears the refresh token.
#[must_use]
pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Lax{}",
        if_secure(secure)
    )
}

fn if_secure(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
}

/// Attach a `Set-Cookie` header to a response.
pub fn attach(response: &mut axum::response::Response, value: &str) {
    if let Ok(header) = value.parse() {
        response.headers_mut().append(SET_COOKIE, header);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn headers_with_cookie(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::COOKIE, value.parse().unwrap());
        headers
    }

    #[test]
    fn parses_refresh_cookie_among_others() {
        let headers = headers_with_cookie("session=x; refresh_token=abc123; theme=dark");
        assert_eq!(refresh_from_headers(&headers).as_deref(), Some("abc123"));
    }

    #[test]
    fn missing_or_empty_cookie_is_none() {
        assert_eq!(refresh_from_headers(&HeaderMap::new()), None);
        assert_eq!(
            refresh_from_headers(&headers_with_cookie("refresh_token=")),
            None
        );
        assert_eq!(refresh_from_headers(&headers_with_cookie("other=1")), None);
    }

    #[test]
    fn cookie_attributes_are_hardened() {
        let value = set_cookie("v", 86_400, false);
        assert!(value.contains("Path=/auth"));
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("SameSite=Lax"));
        assert!(value.contains("Max-Age=86400"));
        assert!(!value.contains("Secure"));

        assert!(set_cookie("v", 1, true).contains("Secure"));
        assert!(clear_cookie(false).contains("Max-Age=0"));
        assert!(clear_cookie(true).contains("Secure"));
    }
}
