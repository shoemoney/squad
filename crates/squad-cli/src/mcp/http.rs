//! Authenticated Streamable HTTP for native MCP clients and a TLS reverse proxy.

mod oauth;

use std::{
    future::IntoFuture,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, uri::Authority},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use reqwest::Url;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use subtle::ConstantTimeEq;

use super::SquadMcp;

const MCP_PATH: &str = "/mcp";
const METADATA_PATH: &str = "/.well-known/oauth-protected-resource/mcp";
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub struct HttpOptions {
    pub listen: SocketAddr,
    /// The exact client-facing /mcp URL. The TLS proxy must preserve its Host.
    pub public_url: Url,
    pub auth: AuthOptions,
}

pub enum AuthOptions {
    /// File containing a single bearer token, readable only by its owner.
    BearerFile(PathBuf),
    OAuth(OAuthOptions),
}

/// An external OAuth authorization server or OIDC provider owns login, client
/// registration, and access-token issuance. Configure a dedicated audience for
/// Squad's JWT access tokens, separate from every OAuth client's ID-token audience.
pub struct OAuthOptions {
    pub issuer: Url,
    pub audience: String,
    pub owner_subject: String,
}

enum Auth {
    Bearer(Vec<u8>),
    OAuth {
        verifier: Box<oauth::Verifier>,
        challenge: HeaderValue,
    },
}

pub async fn serve(server: SquadMcp, mut options: HttpOptions) -> Result<(), String> {
    validate_options(&options)?;
    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .map_err(|error| format!("could not bind MCP HTTP listener: {}", error.kind()))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read MCP HTTP listener address: {}", error.kind()))?;

    // An inferred loopback URL can use an ephemeral port for local clients.
    if options.public_url.scheme() == "http" && options.public_url.port() == Some(0) {
        options
            .public_url
            .set_port(Some(address.port()))
            .map_err(|()| "could not set the MCP HTTP listener port".to_owned())?;
    }

    options.listen = address;
    let config = StreamableHttpServerConfig::default();
    let shutdown = config.cancellation_token.clone();
    let app = router(server, &options, config).await?;
    let stopped = shutdown.clone();
    let serving = axum::serve(listener, app)
        .with_graceful_shutdown(async move { stopped.cancelled().await })
        .into_future();
    tokio::pin!(serving);
    eprintln!("Squad MCP HTTP listening on {address}/mcp");

    let result = tokio::select! {
        result = &mut serving => result,
        () = shutdown_signal() => {
            shutdown.cancel();
            tokio::time::timeout(SHUTDOWN_GRACE, serving).await.unwrap_or(Ok(()))
        }
    };
    result.map_err(|error| format!("MCP HTTP server failed: {}", error.kind()))
}

pub async fn router(
    server: SquadMcp,
    options: &HttpOptions,
    config: StreamableHttpServerConfig,
) -> Result<Router, String> {
    validate_options(options)?;
    let auth = match &options.auth {
        AuthOptions::BearerFile(path) => Auth::Bearer(read_token_file(path)?),
        AuthOptions::OAuth(settings) => {
            let verifier = oauth::Verifier::discover(settings).await?;
            let mut metadata_url = options.public_url.clone();
            metadata_url.set_path(METADATA_PATH);
            let challenge =
                HeaderValue::from_str(&format!("Bearer resource_metadata=\"{metadata_url}\""))
                    .map_err(|_| "invalid MCP resource metadata URL".to_owned())?;
            Auth::OAuth {
                verifier: Box::new(verifier),
                challenge,
            }
        }
    };
    authenticated_router(server, options, config, auth)
}

fn authenticated_router(
    server: SquadMcp,
    options: &HttpOptions,
    config: StreamableHttpServerConfig,
    auth: Auth,
) -> Result<Router, String> {
    let endpoint = Endpoint::new(&options.public_url)?;
    let mut app = Router::new();
    if let Auth::OAuth { verifier, .. } = &auth {
        let metadata = serde_json::json!({
            "resource": options.public_url.as_str(),
            "authorization_servers": [verifier.issuer()],
            "bearer_methods_supported": ["header"]
        });
        app = app.route(METADATA_PATH, get(move || async move { Json(metadata) }));
    }
    let config = config
        .with_allowed_hosts([endpoint.host.clone()])
        .with_allowed_origins([endpoint.origin.clone()])
        .with_max_request_body_bytes(MAX_REQUEST_BYTES);
    let service: StreamableHttpService<_, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(server.clone()), Arc::default(), config);
    let mcp = Router::new()
        .route_service(MCP_PATH, service)
        .route_layer(middleware::from_fn_with_state(Arc::new(auth), authorize));

    Ok(app
        .merge(mcp)
        .layer(middleware::from_fn_with_state(endpoint, check_endpoint)))
}

fn validate_options(options: &HttpOptions) -> Result<(), String> {
    let url = &options.public_url;
    let is_local_http = url.scheme() == "http"
        && matches!(&options.auth, AuthOptions::BearerFile(_))
        && is_loopback_url(url);
    if url.path() != MCP_PATH || !plain_url(url) || !(url.scheme() == "https" || is_local_http) {
        return Err(
            "MCP public URL must be an HTTPS /mcp URL (HTTP is allowed for loopback bearer mode)"
                .to_owned(),
        );
    }
    if !options.listen.ip().is_loopback() {
        return Err("MCP HTTP must listen on loopback behind an HTTPS reverse proxy".to_owned());
    }
    if is_local_http && url.port_or_known_default() != Some(options.listen.port()) {
        return Err("loopback MCP public URL port must match the listener".to_owned());
    }
    Ok(())
}

fn plain_url(url: &Url) -> bool {
    url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn is_loopback_url(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

#[derive(Clone)]
struct Endpoint {
    host: String,
    port: u16,
    scheme_port: u16,
    origin: String,
}

impl Endpoint {
    fn new(url: &Url) -> Result<Self, String> {
        let host = url
            .host_str()
            .ok_or("missing MCP public URL hostname")?
            .to_owned();
        let scheme_port = if url.scheme() == "https" { 443 } else { 80 };
        Ok(Self {
            host,
            port: url.port().unwrap_or(scheme_port),
            scheme_port,
            origin: url.origin().ascii_serialization(),
        })
    }

    fn accepts_authority(&self, authority: &Authority) -> bool {
        authority.as_str().split_once('@').is_none()
            && authority.host().eq_ignore_ascii_case(&self.host)
            && match authority.port_u16() {
                Some(port) => port == self.port,
                None => {
                    self.port == self.scheme_port
                        && authority.as_str().eq_ignore_ascii_case(&self.host)
                }
            }
    }
}

async fn check_endpoint(
    State(endpoint): State<Endpoint>,
    request: Request,
    next: Next,
) -> Response {
    let hosts = request.headers().get_all(header::HOST);
    let mut hosts = hosts.iter();
    let host = hosts.next();
    let authority = request.uri().authority();
    if hosts.next().is_some()
        || (host.is_none() && authority.is_none())
        || host.is_some_and(|value| {
            !value
                .to_str()
                .ok()
                .and_then(|host| host.parse::<Authority>().ok())
                .is_some_and(|host| endpoint.accepts_authority(&host))
        })
        || authority.is_some_and(|authority| !endpoint.accepts_authority(authority))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let origins = request.headers().get_all(header::ORIGIN);
    let mut origins = origins.iter();
    if origins
        .next()
        .is_some_and(|origin| origin.as_bytes() != endpoint.origin.as_bytes())
        || origins.next().is_some()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.uri().query().is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    next.run(request).await
}

async fn authorize(State(auth): State<Arc<Auth>>, mut request: Request, next: Next) -> Response {
    let authenticated = match bearer(request.headers()) {
        Some(token) => match auth.as_ref() {
            Auth::Bearer(expected) => bool::from(token.as_bytes().ct_eq(expected)),
            Auth::OAuth { verifier, .. } => verifier.accepts(token).await,
        },
        None => false,
    };
    if !authenticated {
        let challenge = match auth.as_ref() {
            Auth::Bearer(_) => HeaderValue::from_static("Bearer"),
            Auth::OAuth { challenge, .. } => challenge.clone(),
        };
        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::WWW_AUTHENTICATE, challenge),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
        )
            .into_response();
    }
    request.headers_mut().remove(header::AUTHORIZATION);
    next.run(request).await
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let values = headers.get_all(header::AUTHORIZATION);
    let mut values = values.iter();
    let (scheme, token) = values.next()?.to_str().ok()?.split_once(' ')?;
    let token = token.trim_start_matches(' ');
    (values.next().is_none() && scheme.eq_ignore_ascii_case("bearer") && valid_token(token))
        .then_some(token)
}

fn valid_token(token: &str) -> bool {
    let value = token.trim_end_matches('=');
    !value.is_empty()
        && (32..=MAX_TOKEN_BYTES).contains(&token.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
}

#[cfg(unix)]
fn read_token_file(path: &Path) -> Result<Vec<u8>, String> {
    use std::{
        fs::OpenOptions,
        io::Read,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "could not open MCP bearer token file".to_owned())?;
    let metadata = file
        .metadata()
        .map_err(|_| "could not inspect MCP bearer token file".to_owned())?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("MCP bearer token file must be a private regular file (chmod 600)".to_owned());
    }
    let mut contents = String::new();
    file.take((MAX_TOKEN_BYTES + 3) as u64)
        .read_to_string(&mut contents)
        .map_err(|_| "could not read MCP bearer token file".to_owned())?;
    let token = contents
        .strip_suffix("\r\n")
        .or_else(|| contents.strip_suffix('\n'))
        .unwrap_or(&contents);
    if !valid_token(token) {
        return Err(
            "MCP bearer token file must contain one valid bearer token of at least 32 bytes"
                .to_owned(),
        );
    }
    Ok(token.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn read_token_file(_path: &Path) -> Result<Vec<u8>, String> {
    Err("MCP bearer token files require Unix file permissions".to_owned())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    if let Ok(mut terminate) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
        return;
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_requires_one_bearer_header() {
        let mut headers = HeaderMap::new();
        assert!(bearer(&headers).is_none());
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bEaReR  0123456789abcdef0123456789abcdef"),
        );
        assert_eq!(bearer(&headers), Some("0123456789abcdef0123456789abcdef"));
        headers.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer other"),
        );
        assert!(bearer(&headers).is_none());
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer first second"),
        );
        assert!(bearer(&headers).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn bearer_token_requires_at_least_32_bytes() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        for length in [1, 31, 32, 64] {
            fs::write(&path, "a".repeat(length)).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(valid_token(&"a".repeat(length)), length >= 32);
            if length < 32 {
                assert_eq!(
                    read_token_file(&path).unwrap_err(),
                    "MCP bearer token file must contain one valid bearer token of at least 32 bytes"
                );
            } else {
                assert_eq!(read_token_file(&path).unwrap().len(), length);
            }
        }
    }

    #[test]
    fn authority_requires_the_public_hostname_and_port() {
        let endpoint = Endpoint::new(&Url::parse("https://squad.example/mcp").unwrap()).unwrap();
        assert!(endpoint.accepts_authority(&"SQUAD.EXAMPLE".parse().unwrap()));
        assert!(endpoint.accepts_authority(&"squad.example:443".parse().unwrap()));
        for value in [
            "rebind.example",
            "squad.example:444",
            "squad.example:65536",
            "user@squad.example",
        ] {
            assert!(
                !value
                    .parse()
                    .is_ok_and(|authority| endpoint.accepts_authority(&authority))
            );
        }
        let endpoint = Endpoint::new(&Url::parse("http://[::1]:8787/mcp").unwrap()).unwrap();
        assert!(endpoint.accepts_authority(&"[::1]:8787".parse().unwrap()));
        assert!(!endpoint.accepts_authority(&"[::1]".parse().unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn bearer_file_rejects_public_permissions_and_symlinks() {
        use std::{
            fs,
            os::unix::fs::{PermissionsExt, symlink},
        };

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        fs::write(&path, b"0123456789abcdef0123456789abcdef\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_token_file(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_token_file(&path).unwrap(),
            b"0123456789abcdef0123456789abcdef"
        );
        let link = directory.path().join("link");
        symlink(path, &link).unwrap();
        assert!(read_token_file(&link).is_err());
    }
}
