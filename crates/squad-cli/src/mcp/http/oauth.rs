use std::{sync::Arc, time::Duration};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, EllipticCurve, Jwk, KeyAlgorithm, KeyOperations, PublicKeyUse},
};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::{sync::Mutex, time::Instant};

use super::{OAuthOptions, plain_url};

const MAX_PROVIDER_DOCUMENT: usize = 256 * 1024;
const KEY_LIFETIME: Duration = Duration::from_secs(300);
const RETRY_DELAY: Duration = Duration::from_secs(30);

pub(super) struct Verifier {
    issuer: String,
    client: Client,
    jwks_url: Url,
    keys: Mutex<Cache>,
    refresh: Mutex<()>,
    rs256: Validation,
    es256: Validation,
}

struct Cache {
    // Access fails closed if a refresh fails after the previous keys expire.
    retry_at: Instant,
    early_refresh_at: Instant,
    keys: Option<Arc<Vec<SigningKey>>>,
}

struct SigningKey {
    kid: Option<String>,
    algorithm: Algorithm,
    key: DecodingKey,
}

#[derive(Deserialize)]
struct ProviderMetadata {
    issuer: String,
    jwks_uri: String,
}

#[derive(Deserialize)]
struct KeySet {
    keys: Vec<Value>,
}

#[derive(Deserialize)]
struct AccessClaims {
    iss: String,
}

impl Verifier {
    pub(super) async fn discover(options: &OAuthOptions) -> Result<Self, String> {
        if options.issuer.scheme() != "https" || !plain_url(&options.issuer) {
            return Err("MCP OAuth issuer must be an HTTPS URL".to_owned());
        }
        if options.audience.trim().is_empty() || options.owner_subject.trim().is_empty() {
            return Err("MCP OAuth audience and owner subject are required".to_owned());
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| "could not create MCP OAuth discovery client".to_owned())?;
        let provider = provider_metadata(&client, &options.issuer).await?;
        // Url inserts a slash on a bare origin. Keep the provider's original
        // issuer string for comparisons against signed claims and public metadata.
        let configured = options.issuer.as_str();
        if provider.issuer != configured
            && !(options.issuer.path() == "/"
                && provider.issuer == configured.trim_end_matches('/'))
        {
            return Err("MCP OAuth discovery issuer did not match configured issuer".to_owned());
        }
        let jwks_url = Url::parse(&provider.jwks_uri)
            .ok()
            .filter(|url| url.scheme() == "https" && plain_url(url))
            .ok_or("MCP OAuth discovery must advertise an HTTPS JWKS URL")?;

        // Only trusted discovery can choose this endpoint. JWT headers and
        // claims never influence discovery URLs or key fetches.
        let keys = fetch_keys(&client, &jwks_url).await?;
        let rs256 = validation(Algorithm::RS256, &provider.issuer, options);
        let es256 = validation(Algorithm::ES256, &provider.issuer, options);
        Ok(Self {
            issuer: provider.issuer,
            client,
            jwks_url,
            keys: Mutex::new(Cache {
                retry_at: Instant::now() + KEY_LIFETIME,
                early_refresh_at: Instant::now(),
                keys: Some(Arc::new(keys)),
            }),
            refresh: Mutex::new(()),
            rs256,
            es256,
        })
    }

    pub(super) fn issuer(&self) -> &str {
        &self.issuer
    }

    pub(super) async fn accepts(&self, token: &str) -> bool {
        let Ok(header) = decode_header(token) else {
            return false;
        };
        let validation = match header.alg {
            Algorithm::RS256 => &self.rs256,
            Algorithm::ES256 => &self.es256,
            _ => return false,
        };
        if header.crit.is_some()
            || header.typ.as_deref().is_some_and(|typ| {
                !["JWT", "at+jwt", "application/at+jwt"]
                    .iter()
                    .any(|expected| typ.eq_ignore_ascii_case(expected))
            })
        {
            return false;
        }
        let Some(keys) = self.current_keys(header.kid.as_deref(), header.alg).await else {
            return false;
        };
        let mut matches = keys.iter().filter(|key| {
            key.algorithm == header.alg
                && header
                    .kid
                    .as_ref()
                    .is_none_or(|kid| key.kid.as_ref() == Some(kid))
        });
        matches.next().is_some_and(|key| {
            matches.next().is_none()
                && decode::<AccessClaims>(token, &key.key, validation)
                    .is_ok_and(|verified| verified.claims.iss == self.issuer)
        })
    }

    async fn current_keys(
        &self,
        kid: Option<&str>,
        algorithm: Algorithm,
    ) -> Option<Arc<Vec<SigningKey>>> {
        let mut cache = self.keys.lock().await;
        let now = Instant::now();
        let expired = now >= cache.retry_at;
        let unknown = kid.is_some_and(|kid| {
            cache.keys.as_ref().is_some_and(|keys| {
                !keys
                    .iter()
                    .any(|key| key.kid.as_deref() == Some(kid) && key.algorithm == algorithm)
            })
        });
        let should_refresh = expired || (unknown && now >= cache.early_refresh_at);
        if !should_refresh {
            return cache.keys.clone();
        }
        if expired {
            cache.keys = None;
        }
        // A separate, nonblocking refresh guard coalesces fetches. Known keys
        // remain readable while a rotation fetch is in flight; cancellation
        // releases this guard without stranding the cache.
        let Ok(_refresh) = self.refresh.try_lock() else {
            return cache.keys.clone();
        };
        if expired {
            cache.retry_at = now + RETRY_DELAY;
        }
        cache.early_refresh_at = now + RETRY_DELAY;
        drop(cache);

        let fetched = fetch_keys(&self.client, &self.jwks_url).await;
        let mut cache = self.keys.lock().await;
        if let Ok(keys) = fetched {
            cache.keys = Some(Arc::new(keys));
            cache.retry_at = Instant::now() + KEY_LIFETIME;
        }
        cache.keys.clone()
    }
}

fn validation(algorithm: Algorithm, issuer: &str, options: &OAuthOptions) -> Validation {
    let mut validation = Validation::new(algorithm);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "sub"]);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[&options.audience]);
    validation.sub = Some(options.owner_subject.clone());
    validation.validate_nbf = true;
    validation.leeway = 30;
    validation
}

async fn provider_metadata(client: &Client, issuer: &Url) -> Result<ProviderMetadata, String> {
    let mut oauth_url = issuer.clone();
    oauth_url.set_path(&format!(
        "/.well-known/oauth-authorization-server{}",
        if issuer.path() == "/" {
            ""
        } else {
            issuer.path()
        }
    ));
    let metadata = client
        .get(oauth_url)
        .send()
        .await
        .map_err(|_| "could not fetch MCP OAuth provider metadata".to_owned())?;
    let metadata = if matches!(
        metadata.status(),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
    ) {
        let mut oidc_url = issuer.clone();
        oidc_url.set_path(&format!(
            "{}/.well-known/openid-configuration",
            issuer.path().trim_end_matches('/')
        ));
        client
            .get(oidc_url)
            .send()
            .await
            .map_err(|_| "could not fetch MCP OIDC provider metadata".to_owned())?
    } else {
        metadata
    };
    read_document(metadata)
        .await
        .map_err(|()| "invalid MCP OAuth provider metadata".to_owned())
}

async fn fetch_keys(client: &Client, url: &Url) -> Result<Vec<SigningKey>, String> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| "could not fetch MCP OAuth signing keys".to_owned())?;
    let set: KeySet = read_document(response)
        .await
        .map_err(|()| "invalid MCP OAuth signing key document".to_owned())?;
    let keys: Vec<_> = set.keys.into_iter().filter_map(signing_key).collect();
    if keys.is_empty() {
        return Err("MCP OAuth provider has no supported RS256 or ES256 signing keys".to_owned());
    }
    Ok(keys)
}

fn signing_key(value: Value) -> Option<SigningKey> {
    let jwk: Jwk = serde_json::from_value(value).ok()?;
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| *usage != PublicKeyUse::Signature)
        || jwk
            .common
            .key_operations
            .as_ref()
            .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
    {
        return None;
    }
    let algorithm = match (&jwk.algorithm, jwk.common.key_algorithm) {
        (AlgorithmParameters::RSA(rsa), None | Some(KeyAlgorithm::RS256)) if rsa.n.len() >= 342 => {
            Algorithm::RS256
        }
        (AlgorithmParameters::EllipticCurve(ec), None | Some(KeyAlgorithm::ES256))
            if ec.curve == EllipticCurve::P256 =>
        {
            Algorithm::ES256
        }
        _ => return None,
    };
    let key = DecodingKey::from_jwk(&jwk).ok()?;
    Some(SigningKey {
        kid: jwk.common.key_id,
        algorithm,
        key,
    })
}

async fn read_document<D: DeserializeOwned>(mut response: reqwest::Response) -> Result<D, ()> {
    if !response.status().is_success() {
        return Err(());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if chunk.len() > MAX_PROVIDER_DOCUMENT - bytes.len() {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{EncodingKey, Header, encode, get_current_timestamp};
    use p256::{SecretKey, elliptic_curve::rand_core::OsRng, pkcs8::EncodePrivateKey};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn unknown_kid_refreshes_early_without_blocking_known_keys_and_is_rate_limited() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        let encoding = EncodingKey::from_ec_der(
            SecretKey::random(&mut OsRng)
                .to_pkcs8_der()
                .unwrap()
                .as_bytes(),
        );
        let mut jwk = Jwk::from_encoding_key(&encoding, Algorithm::ES256).unwrap();
        jwk.common.key_id = Some("old".into());
        let old = signing_key(serde_json::to_value(&jwk).unwrap()).unwrap();
        jwk.common.key_id = Some("rotated".into());
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"keys": [jwk]}))
                    .set_delay(Duration::from_millis(200)),
            )
            .mount(&server)
            .await;
        let options = OAuthOptions {
            issuer: Url::parse("https://issuer.example").unwrap(),
            audience: "squad".into(),
            owner_subject: "owner".into(),
        };
        let verifier = Arc::new(Verifier {
            issuer: "https://issuer.example".into(),
            client: Client::new(),
            jwks_url: Url::parse(&server.uri()).unwrap(),
            keys: Mutex::new(Cache {
                retry_at: Instant::now() + KEY_LIFETIME,
                early_refresh_at: Instant::now(),
                keys: Some(Arc::new(vec![old])),
            }),
            refresh: Mutex::new(()),
            rs256: validation(Algorithm::RS256, "https://issuer.example", &options),
            es256: validation(Algorithm::ES256, "https://issuer.example", &options),
        });
        let token = |kid: &str| {
            let mut header = Header::new(Algorithm::ES256);
            header.kid = Some(kid.into());
            encode(&header, &json!({"iss": "https://issuer.example", "aud": "squad", "sub": "owner", "exp": get_current_timestamp() + 300}), &encoding).unwrap()
        };
        let refresh = {
            let verifier = verifier.clone();
            let rotated = token("rotated");
            tokio::spawn(async move { verifier.accepts(&rotated).await })
        };
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.received_requests().await.unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), verifier.accepts(&token("old")))
                .await
                .expect("known keys must not wait for the fetch")
        );
        assert!(
            refresh.await.unwrap(),
            "rotated key must be accepted immediately"
        );
        for _ in 0..3 {
            assert!(!verifier.accepts(&token("unknown")).await);
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        verifier.keys.lock().await.early_refresh_at = Instant::now();
        assert!(!verifier.accepts(&token("unknown")).await);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);

        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        verifier.keys.lock().await.retry_at = Instant::now();
        assert!(
            !verifier.accepts(&token("rotated")).await,
            "expired keys fail closed after fetch failure"
        );
        assert!(!verifier.accepts(&token("rotated")).await);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn accepts_only_owner_access_tokens_for_this_resource_with_30_second_skew() {
        let options = OAuthOptions {
            issuer: Url::parse("https://issuer.example").unwrap(),
            audience: "https://squad.example/mcp".to_owned(),
            owner_subject: "room-owner".to_owned(),
        };
        let issuer = "https://issuer.example";
        let secret = SecretKey::random(&mut OsRng).to_pkcs8_der().unwrap();
        let encoding = EncodingKey::from_ec_der(secret.as_bytes());
        let mut jwk = Jwk::from_encoding_key(&encoding, Algorithm::ES256).unwrap();
        jwk.common.key_id = Some("signing-key".to_owned());
        let key = signing_key(serde_json::to_value(jwk).unwrap()).unwrap();

        let verifier = Verifier {
            issuer: issuer.to_owned(),
            client: Client::new(),
            jwks_url: Url::parse("https://issuer.example/jwks").unwrap(),
            keys: Mutex::new(Cache {
                retry_at: Instant::now() + KEY_LIFETIME,
                early_refresh_at: Instant::now(),
                keys: Some(Arc::new(vec![key])),
            }),
            refresh: Mutex::new(()),
            rs256: validation(Algorithm::RS256, issuer, &options),
            es256: validation(Algorithm::ES256, issuer, &options),
        };
        let mut header = Header::new(Algorithm::ES256);
        header.typ = Some("at+jwt".to_owned());
        header.kid = Some("signing-key".to_owned());
        let now = get_current_timestamp();
        let claims = json!({
            "iss": issuer,
            "aud": [&options.audience, "other-audience"],
            "sub": &options.owner_subject,
            "nbf": now.saturating_sub(1),
            "exp": now + 300
        });
        assert!(
            verifier
                .accepts(&encode(&header, &claims, &encoding).unwrap())
                .await
        );
        for (name, value) in [("exp", json!(now - 10)), ("nbf", json!(now + 10))] {
            let mut skewed = claims.clone();
            skewed[name] = value;
            assert!(
                verifier
                    .accepts(&encode(&header, &skewed, &encoding).unwrap())
                    .await
            );
        }
        for (name, value) in [
            ("iss", json!("https://other-issuer.example")),
            ("iss", json!([issuer])),
            ("aud", json!("oidc-client-id")),
            ("sub", json!("different-user")),
            ("exp", json!(now - 60)),
            ("nbf", json!(now + 300)),
        ] {
            let mut changed = claims.clone();
            changed[name] = value;
            assert!(
                !verifier
                    .accepts(&encode(&header, &changed, &encoding).unwrap())
                    .await,
                "{name}"
            );
        }

        let unrelated = SecretKey::random(&mut OsRng).to_pkcs8_der().unwrap();
        let unrelated = EncodingKey::from_ec_der(unrelated.as_bytes());
        assert!(
            !verifier
                .accepts(&encode(&header, &claims, &unrelated).unwrap())
                .await
        );
        verifier.keys.lock().await.early_refresh_at = Instant::now() + RETRY_DELAY;
        header.kid = Some("unpublished-key".to_owned());
        header.jku = Some("https://untrusted.example/jwks".to_owned());
        assert!(
            !verifier
                .accepts(&encode(&header, &claims, &encoding).unwrap())
                .await
        );
        // Exercise the real HTTP routing and auth middleware with trusted keys
        // injected locally; discovery itself remains HTTPS-only in production.
        use super::super::{Auth, AuthOptions, HttpOptions, authenticated_router};
        use crate::mcp::{SquadMcp, backend::Backend};
        let server = SquadMcp::new(Arc::new(Backend::new(Err("unconfigured".into()), false)));
        let options_http = HttpOptions {
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: Url::parse("https://squad.example/mcp").unwrap(),
            auth: AuthOptions::OAuth(options),
        };
        header.kid = Some("signing-key".to_owned());
        header.jku = None;
        let token = encode(&header, &claims, &encoding).unwrap();
        let config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
        let app = authenticated_router(server, &options_http, config, Auth::OAuth {
            verifier: Box::new(verifier),
            challenge: "Bearer resource_metadata=\"https://squad.example/.well-known/oauth-protected-resource/mcp\"".parse().unwrap(),
        }).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::new();
        let metadata = client
            .get(format!("{base}/.well-known/oauth-protected-resource/mcp"))
            .header("host", "squad.example")
            .send()
            .await
            .unwrap();
        assert_eq!(metadata.status(), StatusCode::OK);
        assert_eq!(
            metadata.json::<Value>().await.unwrap()["authorization_servers"],
            json!([issuer])
        );
        let denied = client
            .post(format!("{base}/mcp"))
            .header("host", "squad.example")
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        assert!(
            denied.headers()["www-authenticate"]
                .to_str()
                .unwrap()
                .contains("resource_metadata")
        );
        let denied = client
            .post(format!("{base}/mcp"))
            .header("host", "squad.example")
            .bearer_auth("invalid-jwt")
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let initialized = client
            .post(format!("{base}/mcp"))
            .header("host", "squad.example")
            .header("accept", "application/json, text/event-stream")
            .bearer_auth(token)
            .json(
                &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-11-25","capabilities":{},
                    "clientInfo":{"name":"oauth-test","version":"1"}
                }}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(initialized.status(), StatusCode::OK);
        assert!(initialized.text().await.unwrap().contains("serverInfo"));
        task.abort();
        let _ = task.await;
    }
}
