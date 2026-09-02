//! Pre-signed media uploads against MinIO/S3 (ADR-009).

use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;

/// Thin wrapper for minting pre-signed PUT URLs.
#[derive(Clone)]
pub struct MediaPresigner {
    client: Client,
    bucket: String,
    expiry_secs: u64,
}

/// A pre-signed upload grant.
#[derive(Debug, serde::Serialize)]
pub struct PresignedUpload {
    /// The pre-signed PUT URL.
    pub upload_url: String,
    /// Object key the client must upload to.
    pub object_key: String,
    /// Lifetime of the URL in seconds.
    pub expires_in: u64,
}

impl MediaPresigner {
    /// Build a presigner for a static-credential MinIO/S3 endpoint with
    /// forced path-style addressing.
    ///
    /// # Errors
    ///
    /// Never fails today; the `Result` keeps the signature stable for
    /// credential providers that can fail.
    pub async fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        expiry_secs: u64,
    ) -> Self {
        use aws_config::BehaviorVersion;
        use aws_sdk_s3::config::Credentials;
        use aws_sdk_s3::config::Region;

        let credentials = Credentials::new(access_key, secret_key, None, None, "static");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials.clone())
            .region(Region::new("us-east-1"))
            .load()
            .await;
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .endpoint_url(endpoint)
            .force_path_style(true)
            .credentials_provider(credentials)
            .build();
        Self {
            client: Client::from_conf(s3_config),
            bucket: bucket.to_string(),
            expiry_secs,
        }
    }

    /// Bucket the presigner targets.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Underlying S3 client (tests and head-object checks).
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Mint a pre-signed PUT URL for `object_key`.
    ///
    /// When `content_type` is provided it becomes part of the signature, so
    /// the client must send the identical `Content-Type` header.
    ///
    /// # Errors
    ///
    /// Fails on presigning errors (bad configuration, clock issues).
    pub async fn presign_put(
        &self,
        object_key: &str,
        content_type: Option<&str>,
    ) -> anyhow::Result<PresignedUpload> {
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key);
        if let Some(content_type) = content_type {
            request = request.content_type(content_type);
        }
        let presigned = request
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(self.expiry_secs))
                    .build()?,
            )
            .await?;
        Ok(PresignedUpload {
            upload_url: presigned.uri().to_string(),
            object_key: object_key.to_string(),
            expires_in: self.expiry_secs,
        })
    }
}
