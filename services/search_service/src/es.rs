//! ElasticSearch catalog: index management, document I/O, and the search
//! query (fuzzy + boosted + autocomplete, per the product decision).

use elasticsearch::Elasticsearch;
use elasticsearch::SearchParts;
use elasticsearch::http::transport::Transport;
use elasticsearch::indices::IndicesCreateParts;
use elasticsearch::params::Refresh;

/// Index holding the song catalog.
pub const INDEX: &str = "songs";

/// Build a client for a single-node cluster.
///
/// # Errors
///
/// Fails when the transport cannot be constructed.
pub fn client(url: &str) -> anyhow::Result<Elasticsearch> {
    let transport = Transport::single_node(url)?;
    Ok(Elasticsearch::new(transport))
}

/// Create the catalog index with the autocomplete mapping if missing.
///
/// Mapping: `title`/`author` are `text` with edge-ngram `autocomplete`
/// sub-fields (index-time n-grams, standard search analyzer — the classic
/// as-you-type pattern); identifiers are keywords.
///
/// # Errors
///
/// Fails on transport/cluster errors other than "already exists".
pub async fn ensure_index(client: &Elasticsearch) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "settings": {
            "analysis": {
                "filter": {
                    "autocomplete_filter": { "type": "edge_ngram", "min_gram": 2, "max_gram": 15 }
                },
                "analyzer": {
                    "autocomplete": {
                        "type": "custom",
                        "tokenizer": "standard",
                        "filter": ["lowercase", "autocomplete_filter"]
                    }
                }
            }
        },
        "mappings": {
            "properties": {
                "song_id": { "type": "keyword" },
                "label_id": { "type": "keyword" },
                "title": {
                    "type": "text",
                    "fields": {
                        "autocomplete": {
                            "type": "text",
                            "analyzer": "autocomplete",
                            "search_analyzer": "standard"
                        }
                    }
                },
                "author": {
                    "type": "text",
                    "fields": {
                        "autocomplete": {
                            "type": "text",
                            "analyzer": "autocomplete",
                            "search_analyzer": "standard"
                        }
                    }
                },
                "length_seconds": { "type": "integer" },
                "box_art_key": { "type": "keyword" },
                "audio_preview_key": { "type": "keyword" },
                "created_at": { "type": "date" }
            }
        }
    });
    let response = client
        .indices()
        .create(IndicesCreateParts::Index(INDEX))
        .body(body)
        .send()
        .await?;

    if response.status_code().is_success() {
        tracing::info!(index = INDEX, "created songs index");
    } else {
        // 400 resource_already_exists_exception is the expected rerun path.
        let status = response.status_code();
        let text = response.text().await.unwrap_or_default();
        anyhow::ensure!(
            status.as_u16() == 400 && text.contains("resource_already_exists_exception"),
            "unexpected index creation result {status}: {text}"
        );
    }

    // Merge the date mapping into pre-existing indexes (create-time
    // mappings only apply to fresh ones); adding a field is idempotent.
    let put = client
        .indices()
        .put_mapping(elasticsearch::indices::IndicesPutMappingParts::Index(&[
            INDEX,
        ]))
        .body(serde_json::json!({
            "properties": { "created_at": { "type": "date" } }
        }))
        .send()
        .await?;
    anyhow::ensure!(
        put.status_code().is_success(),
        "created_at mapping merge failed: {}",
        put.status_code()
    );
    Ok(())
}

/// Index (upsert) a song record; idempotent by song id.
///
/// # Errors
///
/// Fails on transport/cluster errors.
pub async fn upsert_song(
    client: &Elasticsearch,
    song: &licensing_core::SongRecord,
) -> anyhow::Result<()> {
    let body = serde_json::to_value(song)?;
    let response = client
        .index(elasticsearch::IndexParts::IndexId(
            INDEX,
            &song.song_id.to_string(),
        ))
        .body(body)
        .refresh(Refresh::False)
        .send()
        .await?;
    anyhow::ensure!(
        response.status_code().is_success(),
        "upsert failed: {}",
        response.status_code()
    );
    Ok(())
}

/// Remove a song from the catalog; a missing document is fine.
///
/// # Errors
///
/// Fails on transport/cluster errors.
pub async fn delete_song(client: &Elasticsearch, song_id: uuid::Uuid) -> anyhow::Result<()> {
    let response = client
        .delete(elasticsearch::DeleteParts::IndexId(
            INDEX,
            &song_id.to_string(),
        ))
        .refresh(Refresh::False)
        .send()
        .await?;
    let status = response.status_code().as_u16();
    anyhow::ensure!(
        status == 200 || status == 404,
        "delete failed with {status}"
    );
    Ok(())
}

/// One search hit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SongHit {
    /// Song id.
    pub song_id: uuid::Uuid,
    /// Owning label.
    pub label_id: uuid::Uuid,
    /// Title.
    pub title: String,
    /// Author.
    pub author: String,
    /// Length in seconds.
    pub length_seconds: u32,
    /// Box art key, when uploaded.
    pub box_art_key: Option<String>,
    /// Audio preview key, when uploaded.
    pub audio_preview_key: Option<String>,
    /// When the song entered the catalog (absent on pre-upgrade docs).
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A page of search results.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchPage {
    /// Total documents matching the query.
    pub total: u64,
    /// Hits for this page.
    pub hits: Vec<SongHit>,
}

/// Query the catalog: fuzzy `multi_match` over title (boosted 3x) and
/// author, plus strict prefix matching through the autocomplete sub-fields.
///
/// # Errors
///
/// Fails on transport/cluster errors or malformed responses.
pub async fn search(
    client: &Elasticsearch,
    query: &str,
    from: i64,
    size: i64,
) -> anyhow::Result<SearchPage> {
    let body = serde_json::json!({
        "from": from,
        "size": size,
        "query": {
            "bool": {
                "should": [
                    {
                        "multi_match": {
                            "query": query,
                            "fields": ["title^3", "author"],
                            "fuzziness": "AUTO"
                        }
                    },
                    {
                        "multi_match": {
                            "query": query,
                            "fields": ["title.autocomplete^3", "author.autocomplete"]
                        }
                    }
                ],
                "minimum_should_match": 1
            }
        }
    });
    let response = client
        .search(SearchParts::Index(&[INDEX]))
        .body(body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status_code().is_success(),
        "search failed: {}",
        response.status_code()
    );
    let parsed: serde_json::Value = response.json().await?;
    let total = parsed["hits"]["total"]["value"].as_u64().unwrap_or(0);
    let hits = parsed["hits"]["hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|hit| {
                    let src = &hit["_source"];
                    serde_json::from_value::<SongHit>(src.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SearchPage { total, hits })
}

/// Newest-first catalog listing for pre-search suggestions. Docs without a
/// `created_at` (pre-upgrade) sort last; the song_id tiebreak keeps the
/// order stable across pages.
///
/// # Errors
///
/// Fails on transport/cluster errors or malformed responses.
pub async fn newest(client: &Elasticsearch, size: i64) -> anyhow::Result<SearchPage> {
    let body = serde_json::json!({
        "size": size,
        "query": { "match_all": {} },
        "sort": [
            { "created_at": { "order": "desc", "missing": "_last" } },
            { "song_id": "asc" }
        ]
    });
    let response = client
        .search(SearchParts::Index(&[INDEX]))
        .body(body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status_code().is_success(),
        "newest query failed: {}",
        response.status_code()
    );
    let parsed: serde_json::Value = response.json().await?;
    let total = parsed["hits"]["total"]["value"].as_u64().unwrap_or(0);
    let hits = parsed["hits"]["hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|hit| {
                    let src = &hit["_source"];
                    serde_json::from_value::<SongHit>(src.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SearchPage { total, hits })
}

/// Fetch one document by song id.
///
/// # Errors
///
/// Returns `Ok(None)` when the document is absent; transport errors
/// propagate.
pub async fn get_song(
    client: &Elasticsearch,
    song_id: uuid::Uuid,
) -> anyhow::Result<Option<SongHit>> {
    let response = client
        .get(elasticsearch::GetParts::IndexId(
            INDEX,
            &song_id.to_string(),
        ))
        .send()
        .await?;
    if response.status_code().as_u16() == 404 {
        return Ok(None);
    }
    anyhow::ensure!(
        response.status_code().is_success(),
        "get failed: {}",
        response.status_code()
    );
    let parsed: serde_json::Value = response.json().await?;
    Ok(serde_json::from_value(parsed["_source"].clone()).ok())
}
