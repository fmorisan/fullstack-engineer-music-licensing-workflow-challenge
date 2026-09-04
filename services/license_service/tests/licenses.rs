//! Integration tests for license creation and reads: real Postgres via
//! testcontainers, upstream movie/song validation against in-process axum
//! stubs that honor the propagated bearer token.

use axum::Json;
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::routing::get;
use http_body_util::BodyExt;
use license_service::state::AppState;
use license_service::upstream::Upstream;
use license_service::{build_router, db};
use serde_json::{Value, json};
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

fn token(role: licensing_core::Role, org: Option<Uuid>) -> String {
    test_support::access_token(&fixture().pair, role, org)
}

/// An in-process stub of movie_service + song_service.
struct Stubs {
    movie_url: String,
    song_url: String,
    movie_id: Uuid,
    scene_screen_time: i32,
    song_id: Uuid,
    label_org: Uuid,
    studio_org: Uuid,
}

impl Stubs {
    async fn start() -> Self {
        let studio_org = Uuid::now_v7();
        let label_org = Uuid::now_v7();
        let movie_id = Uuid::now_v7();
        let song_id = Uuid::now_v7();

        let known_movie = movie_id;
        let known_studio = studio_org;
        let movie_app = axum::Router::new().route(
            "/movies/{id}",
            get(move |Path(id): Path<Uuid>| async move {
                if id != known_movie {
                    return (StatusCode::NOT_FOUND, Json(Value::Null));
                }
                (
                    StatusCode::OK,
                    Json(json!({
                        "movie": { "id": known_movie.to_string(), "studio_id": known_studio.to_string() },
                        "scenes": [
                            { "scene_number": 1, "screen_time_seconds": 120, "start_time_seconds": 0, "end_time_seconds": 120 },
                            { "scene_number": 2, "screen_time_seconds": 60, "start_time_seconds": 120, "end_time_seconds": 180 },
                        ]
                    })),
                )
            }),
        );
        let movie_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let movie_url = format!("http://{}", movie_listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(movie_listener, movie_app).await.unwrap() });

        let known_song = song_id;
        let known_label = label_org;
        let song_app = axum::Router::new().route(
            "/songs/{id}",
            get(move |Path(id): Path<Uuid>| async move {
                if id != known_song {
                    return (StatusCode::NOT_FOUND, Json(Value::Null));
                }
                (
                    StatusCode::OK,
                    Json(json!({
                        "id": known_song.to_string(),
                        "label_id": known_label.to_string(),
                        "title": "Stub Song", "author": "Stub Author", "length_seconds": 200
                    })),
                )
            }),
        );
        let song_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let song_url = format!("http://{}", song_listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(song_listener, song_app).await.unwrap() });

        Self {
            movie_url,
            song_url,
            movie_id,
            scene_screen_time: 120,
            song_id,
            label_org,
            studio_org,
        }
    }
}

use axum::extract::Path;

async fn setup(stubs: &Stubs) -> AppState {
    let url = test_support::provision_database("license").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let upstream = Upstream::new(&stubs.movie_url, &stubs.song_url).unwrap();
    AppState::new(pool, upstream)
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let response = build_router(state.clone(), fixture().auth.clone())
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

async fn req(
    state: &AppState,
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
    send(state, request).await
}

fn create_body(stubs: &Stubs) -> Value {
    json!({
        "movie_id": stubs.movie_id.to_string(),
        "scene_number": 1,
        "song_id": stubs.song_id.to_string(),
        "start_time_seconds": 0,
        "end_time_seconds": 30,
        "license_fee_cents": 150_000,
    })
}

#[tokio::test]
async fn studios_create_offers_with_upstream_validation() {
    let stubs = Stubs::start().await;
    let state = setup(&stubs).await;
    let studio_token = token(licensing_core::Role::Studio, Some(stubs.studio_org));

    let (status, body) = req(
        &state,
        "POST",
        "/licenses",
        &studio_token,
        Some(create_body(&stubs)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["state"], "OFFER");
    assert_eq!(body["license_fee_cents"], 150_000);
    assert_eq!(body["label_id"], stubs.label_org.to_string());
    assert_eq!(body["studio_id"], stubs.studio_org.to_string());
    let license_id = body["id"].as_str().unwrap().to_string();

    // Detail carries the creation log entry.
    let (status, body) = req(
        &state,
        "GET",
        &format!("/licenses/{license_id}"),
        &studio_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["log"].as_array().unwrap().len(), 1);
    assert_eq!(body["log"][0]["action"], "CREATE");
    assert_eq!(body["log"][0]["to_state"], "OFFER");
    assert_eq!(body["log"][0]["from_state"], Value::Null);
}

#[tokio::test]
async fn creation_validates_upstream_context() {
    let stubs = Stubs::start().await;
    let state = setup(&stubs).await;
    let studio_token = token(licensing_core::Role::Studio, Some(stubs.studio_org));

    // Unknown movie -> 404.
    let mut ghost_movie = create_body(&stubs);
    ghost_movie["movie_id"] = json!(Uuid::now_v7().to_string());
    let (status, _) = req(
        &state,
        "POST",
        "/licenses",
        &studio_token,
        Some(ghost_movie),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Another studio's movie (stub 404s foreign movies) -> 404.
    let stranger = token(licensing_core::Role::Studio, Some(Uuid::now_v7()));
    let (status, _) = req(
        &state,
        "POST",
        "/licenses",
        &stranger,
        Some(create_body(&stubs)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Unknown scene -> 404.
    let mut bad_scene = create_body(&stubs);
    bad_scene["scene_number"] = json!(99);
    let (status, _) = req(&state, "POST", "/licenses", &studio_token, Some(bad_scene)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Unknown song -> 404.
    let mut ghost_song = create_body(&stubs);
    ghost_song["song_id"] = json!(Uuid::now_v7().to_string());
    let (status, _) = req(&state, "POST", "/licenses", &studio_token, Some(ghost_song)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Window beyond the scene's screen time -> 422.
    let mut too_long = create_body(&stubs);
    too_long["end_time_seconds"] = json!(stubs.scene_screen_time + 1);
    let (status, body) = req(&state, "POST", "/licenses", &studio_token, Some(too_long)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    // Labels cannot create licenses.
    let label = token(licensing_core::Role::Label, Some(stubs.label_org));
    let (status, _) = req(
        &state,
        "POST",
        "/licenses",
        &label,
        Some(create_body(&stubs)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn studios_and_labels_list_their_licenses() {
    let stubs = Stubs::start().await;
    let state = setup(&stubs).await;
    let studio = token(licensing_core::Role::Studio, Some(stubs.studio_org));
    let label = token(licensing_core::Role::Label, Some(stubs.label_org));

    req(
        &state,
        "POST",
        "/licenses",
        &studio,
        Some(create_body(&stubs)),
    )
    .await;

    let (status, body) = req(
        &state,
        "GET",
        &format!("/licenses?movie_id={}", stubs.movie_id),
        &studio,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (status, body) = req(
        &state,
        "GET",
        &format!("/licenses?movie_id={}&scene_number=1", stubs.movie_id),
        &studio,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (status, body) = req(&state, "GET", "/licenses", &label, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1, "label sees incoming");

    // Stranger label sees nothing.
    let stranger = token(licensing_core::Role::Label, Some(Uuid::now_v7()));
    let (status, body) = req(&state, "GET", "/licenses", &stranger, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Studio must scope by movie.
    let (status, _) = req(&state, "GET", "/licenses", &studio, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Detail is party-only.
    let license_id = {
        let (_, body) = req(
            &state,
            "GET",
            &format!("/licenses?movie_id={}", stubs.movie_id),
            &studio,
            None,
        )
        .await;
        body[0]["id"].as_str().unwrap().to_string()
    };
    let stranger_studio = token(licensing_core::Role::Studio, Some(Uuid::now_v7()));
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/{license_id}"),
        &stranger_studio,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn movie_context_shows_all_licenses_to_party_labels_only() {
    let stubs = Stubs::start().await;
    let state = setup(&stubs).await;
    let studio = token(licensing_core::Role::Studio, Some(stubs.studio_org));
    let label = token(licensing_core::Role::Label, Some(stubs.label_org));

    // Two offers from the studio, one per scene — both for stubs.label_org.
    for scene in 1..=2 {
        let (status, body) = req(
            &state,
            "POST",
            "/licenses",
            &studio,
            Some(json!({
                "movie_id": stubs.movie_id.to_string(),
                "scene_number": scene,
                "song_id": stubs.song_id.to_string(),
                "start_time_seconds": 0,
                "end_time_seconds": 30,
                "license_fee_cents": 150_000,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "body: {body}");
    }
    // A foreign label's license on the same movie (raw row: the song stub
    // only knows one label; schema contract is pinned elsewhere).
    sqlx::query(
        "INSERT INTO licenses (id, movie_id, scene_number, song_id, studio_id, label_id,
             studio_user_id, state, license_fee_cents, start_time_seconds, end_time_seconds)
         VALUES ($1, $2, 1, $3, $4, $5, $6, 'OFFER', 90000, 30, 60)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(stubs.movie_id)
    .bind(stubs.song_id)
    .bind(stubs.studio_org)
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .execute(state.pool())
    .await
    .unwrap();

    // Party label: every license on the movie, scene-ordered — including
    // the other label's row (the negotiation context).
    let (status, body) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}", stubs.movie_id),
        &label,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 3, "both scenes plus the foreign row: {body}");
    assert_eq!(rows[0]["scene_number"], 1);

    // Strangers: no existence leak.
    let stranger = token(licensing_core::Role::Label, Some(uuid::Uuid::now_v7()));
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}", stubs.movie_id),
        &stranger,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Studios use their own list path instead.
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}", stubs.movie_id),
        &studio,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn relationship_endpoint_answers_for_party_labels() {
    let stubs = Stubs::start().await;
    let state = setup(&stubs).await;
    let studio = token(licensing_core::Role::Studio, Some(stubs.studio_org));
    let label = token(licensing_core::Role::Label, Some(stubs.label_org));

    // No licenses yet: 404.
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}/relationship", stubs.movie_id),
        &label,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = req(
        &state,
        "POST",
        "/licenses",
        &studio,
        Some(create_body(&stubs)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Party: 204. Strangers: 404 (movie_service fails closed on it).
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}/relationship", stubs.movie_id),
        &label,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let stranger = token(licensing_core::Role::Label, Some(uuid::Uuid::now_v7()));
    let (status, _) = req(
        &state,
        "GET",
        &format!("/licenses/movies/{}/relationship", stubs.movie_id),
        &stranger,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
