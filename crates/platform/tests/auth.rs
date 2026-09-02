//! Router-level tests for the platform auth middleware.

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::routing::get;
use http_body_util::BodyExt;
use licensing_core::Role;
use platform::jwt::{self, Claims};
use platform::{AuthenticatedUser, JwtAuth, require_auth};
use std::sync::OnceLock;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    auth: JwtAuth,
    private: String,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let pair = test_support::RsaKeyPair::generate();
        Fixture {
            auth: JwtAuth::from_public_key_pem(pair.public_pem.clone()),
            private: pair.private_pem,
        }
    })
}

fn token_for(role: Role, ttl_secs: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: Uuid::now_v7().to_string(),
        role,
        org_id: Some(Uuid::now_v7()),
        iss: jwt::ISSUER.to_string(),
        iat: now,
        exp: now + ttl_secs,
    };
    jwt::encode(&fixture().private, &claims).unwrap()
}

fn test_router() -> Router {
    async fn whoami(
        axum::Extension(user): axum::Extension<AuthenticatedUser>,
    ) -> Json<serde_json::Value> {
        serde_json::json!({
            "user_id": user.user_id.to_string(),
            "role": user.role.as_str(),
            "org_id": user.org_id.map(|id| id.to_string()),
        })
        .into()
    }
    async fn admin_only(
        axum::Extension(user): axum::Extension<AuthenticatedUser>,
    ) -> Result<Json<serde_json::Value>, platform::AuthError> {
        user.ensure_role(Role::Admin)?;
        Ok(serde_json::json!({ "ok": true }).into())
    }

    Router::new()
        .route("/me", get(whoami))
        .route("/admin", get(admin_only))
        .layer(axum::middleware::from_fn_with_state(
            fixture().auth.clone(),
            require_auth,
        ))
        .with_state(fixture().auth.clone())
}

async fn get_json(path: &str, bearer: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(path);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = test_router()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn valid_token_passes_and_principal_is_available() {
    let token = token_for(Role::Studio, 300);
    let (status, body) = get_json("/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["role"], "STUDIO");
    assert!(body["user_id"].as_str().is_some());
    assert!(body["org_id"].as_str().is_some());
}

#[tokio::test]
async fn missing_or_garbage_tokens_are_unauthorized() {
    let (no_header, _) = get_json("/me", None).await;
    assert_eq!(no_header, StatusCode::UNAUTHORIZED);

    let (garbage, _) = get_json("/me", Some("garbage.token.value")).await;
    assert_eq!(garbage, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_tokens_are_unauthorized() {
    // Well past the 60s validation leeway.
    let (status, _) = get_json("/me", Some(&token_for(Role::Label, -600))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn foreign_key_tokens_are_unauthorized() {
    let other = test_support::RsaKeyPair::generate();
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: Uuid::now_v7().to_string(),
        role: Role::Admin,
        org_id: None,
        iss: jwt::ISSUER.to_string(),
        iat: now,
        exp: now + 300,
    };
    let token = jwt::encode(&other.private_pem, &claims).unwrap();
    let (status, _) = get_json("/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn access_token_query_parameter_is_accepted() {
    let token = token_for(Role::Studio, 300);
    let (status, body) = get_json(&format!("/me?access_token={token}"), None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
}

#[tokio::test]
async fn ensure_role_gates_and_others_get_forbidden() {
    let admin = token_for(Role::Admin, 300);
    let (ok, _) = get_json("/admin", Some(&admin)).await;
    assert_eq!(ok, StatusCode::OK);

    let studio = token_for(Role::Studio, 300);
    let (forbidden, body) = get_json("/admin", Some(&studio)).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN, "body: {body}");
}
