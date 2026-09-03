//! Transition tests: every (state × action × role) combination checked
//! against the licensing-core oracle, plus fee semantics, party
//! authorization, and log growth.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use license_service::state::AppState;
use license_service::upstream::Upstream;
use license_service::{build_router, db};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::OnceLock;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    auth: platform::JwtAuth,
    pair: test_support::RsaKeyPair,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let pair = test_support::RsaKeyPair::generate();
        Fixture {
            auth: platform::JwtAuth::from_public_key_pem(pair.public_pem.clone()),
            pair,
        }
    })
}

struct Ctx {
    state: AppState,
    pool: PgPool,
    studio_org: Uuid,
    label_org: Uuid,
}

fn token(role: licensing_core::Role, org: Uuid) -> String {
    test_support::access_token(&fixture().pair, role, Some(org))
}

async fn setup() -> Ctx {
    let url = test_support::provision_database("license").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    // Upstreams are never dialed by transition tests.
    let upstream = Upstream::new("http://127.0.0.1:9", "http://127.0.0.1:9").unwrap();
    Ctx {
        state: AppState::new(pool.clone(), upstream),
        pool,
        studio_org: Uuid::now_v7(),
        label_org: Uuid::now_v7(),
    }
}

async fn seed_license(ctx: &Ctx, state: licensing_core::LicenseState) -> Uuid {
    let id = licensing_core::new_id();
    // Runtime queries: test-only fixtures don't participate in the offline
    // prepare workflow (which covers lib/bin targets).
    sqlx::query(
        "INSERT INTO licenses
            (id, movie_id, scene_number, song_id, studio_id, label_id,
             studio_user_id, state, license_fee_cents, start_time_seconds, end_time_seconds)
         VALUES ($1, $2, 1, $3, $4, $5, $6, $7, 1000, 0, 30)",
    )
    .bind(id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(ctx.studio_org)
    .bind(ctx.label_org)
    .bind(Uuid::now_v7())
    .bind(state.as_str())
    .execute(&ctx.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO license_log
            (id, license_id, sequence, from_state, to_state, action, actor_user_id, license_fee_cents)
         VALUES ($1, $2, 1, NULL, $3, 'CREATE', $4, 1000)",
    )
    .bind(licensing_core::new_id())
    .bind(id)
    .bind(state.as_str())
    .bind(Uuid::now_v7())
    .execute(&ctx.pool)
    .await
    .unwrap();
    id
}

async fn req(
    ctx: &Ctx,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(serde_json::to_string(&body).unwrap()))
        }
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = build_router(ctx.state.clone(), fixture().auth.clone())
        .oneshot(request)
        .await
        .expect("call");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn act(
    ctx: &Ctx,
    license_id: Uuid,
    role: licensing_core::Role,
    action: licensing_core::LicenseAction,
    fee: Option<i64>,
) -> (StatusCode, Value) {
    let org = if role == licensing_core::Role::Studio {
        ctx.studio_org
    } else {
        ctx.label_org
    };
    let body = json!({ "action": action.as_str(), "license_fee_cents": fee });
    req(
        ctx,
        "PUT",
        &format!("/licenses/{license_id}"),
        &token(role, org),
        Some(body),
    )
    .await
}

async fn log_length(ctx: &Ctx, license_id: Uuid) -> i64 {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM license_log WHERE license_id = $1")
        .bind(license_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    count
}

#[tokio::test]
async fn exhaustive_matrix_matches_the_core_state_machine() {
    use licensing_core::{LicenseAction as A, LicenseState as S, Role as R};

    for from in S::ALL {
        for action in A::ALL {
            for role in [R::Studio, R::Label] {
                let ctx = setup().await;
                let license_id = seed_license(&ctx, from).await;

                let (status, body) = act(&ctx, license_id, role, action, Some(2000)).await;
                match licensing_core::transition(from, action, role) {
                    Ok(expected) => {
                        assert_eq!(
                            status,
                            StatusCode::OK,
                            "{from:?}/{action:?}/{role:?} must succeed: {body}"
                        );
                        assert_eq!(body["state"], expected.as_str());
                        // Proposal actions move the fee; the rest keep it.
                        let expected_fee = match action {
                            A::Offer | A::CounterOffer => 2000,
                            A::Accept | A::Reject => 1000,
                        };
                        assert_eq!(body["license_fee_cents"], expected_fee);
                        assert_eq!(log_length(&ctx, license_id).await, 2);
                    }
                    Err(licensing_core::TransitionError::ActionForbiddenForRole { .. }) => {
                        assert_eq!(
                            status,
                            StatusCode::FORBIDDEN,
                            "{from:?}/{action:?}/{role:?} must forbid: {body}"
                        );
                        assert_eq!(log_length(&ctx, license_id).await, 1);
                    }
                    Err(licensing_core::TransitionError::IllegalFromState { .. }) => {
                        assert_eq!(
                            status,
                            StatusCode::CONFLICT,
                            "{from:?}/{action:?}/{role:?} must conflict: {body}"
                        );
                        assert_eq!(log_length(&ctx, license_id).await, 1);
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn proposals_require_a_fee() {
    let ctx = setup().await;
    let license_id = seed_license(&ctx, licensing_core::LicenseState::Offer).await;

    let (status, body) = act(
        &ctx,
        license_id,
        licensing_core::Role::Label,
        licensing_core::LicenseAction::CounterOffer,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let (status, _) = act(
        &ctx,
        license_id,
        licensing_core::Role::Label,
        licensing_core::LicenseAction::CounterOffer,
        Some(-5),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn non_parties_get_404_even_with_valid_roles() {
    let ctx = setup().await;
    let license_id = seed_license(&ctx, licensing_core::LicenseState::Offer).await;

    // A studio of ANOTHER org.
    let stranger_studio = Uuid::now_v7();
    let (status, _) = req(
        &ctx,
        "PUT",
        &format!("/licenses/{license_id}"),
        &token(licensing_core::Role::Studio, stranger_studio),
        Some(json!({ "action": "ACCEPT" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A label of ANOTHER org.
    let stranger_label = Uuid::now_v7();
    let (status, _) = req(
        &ctx,
        "PUT",
        &format!("/licenses/{license_id}"),
        &token(licensing_core::Role::Label, stranger_label),
        Some(json!({ "action": "REJECT" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Admins are never a party.
    let (status, _) = req(
        &ctx,
        "PUT",
        &format!("/licenses/{license_id}"),
        &test_support::access_token(&fixture().pair, licensing_core::Role::Admin, None),
        Some(json!({ "action": "REJECT" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn full_negotiation_walks_the_lifecycle_with_log() {
    use licensing_core::LicenseAction as A;
    use licensing_core::Role as R;

    let ctx = setup().await;
    let license_id = seed_license(&ctx, licensing_core::LicenseState::Offer).await;

    let (status, body) = act(&ctx, license_id, R::Label, A::CounterOffer, Some(2500)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "COUNTER_OFFER");
    assert_eq!(body["license_fee_cents"], 2500);

    let (status, body) = act(&ctx, license_id, R::Studio, A::Offer, Some(1800)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "OFFER");

    let (status, body) = act(&ctx, license_id, R::Label, A::Accept, None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["state"], "ACCEPTED");
    assert_eq!(body["license_fee_cents"], 1800, "accept keeps the fee");

    assert_eq!(log_length(&ctx, license_id).await, 4);

    // Terminal is absorbing.
    let (status, _) = act(&ctx, license_id, R::Studio, A::Reject, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
}
