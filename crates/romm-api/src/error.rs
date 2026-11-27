use reqwest::StatusCode;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid server address: {0}")]
    InvalidUrl(String),
    #[error("network error: {}", with_cause(.0))]
    Network(#[from] reqwest::Error),
    #[error("not signed in, or the session expired")]
    Unauthorized,
    #[error("the server refused this ({0})")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("server error {status}: {detail}")]
    Http { status: u16, detail: String },
    #[error("unexpected response: {0}")]
    Decode(String),
}

/// reqwest's message ("error sending request for url ...") hides why; add the innermost
/// cause ("operation timed out", "connection reset", ...).
fn with_cause(e: &reqwest::Error) -> String {
    let mut root: &dyn std::error::Error = e;
    while let Some(s) = root.source() {
        root = s;
    }
    let top = e.to_string();
    let cause = root.to_string();
    if cause == top {
        top
    } else {
        format!("{top}: {cause}")
    }
}

impl Error {
    /// Worth trying again: the network or the server had a problem, not the request.
    pub fn is_transient(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Http { status, .. } => *status >= 500 || *status == 429,
            _ => false,
        }
    }

    /// The RomM `detail` string for a 400 error, if this is one.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Error::Http {
                status: 400,
                detail,
            } => Some(detail),
            _ => None,
        }
    }
}

/// FastAPI error bodies look like `{"detail": "..."}`.
pub(crate) async fn detail_of(resp: reqwest::Response) -> String {
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("detail").map(|d| {
                d.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| d.to_string())
            })
        })
        .unwrap_or(text)
}

pub(crate) async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    Err(match status {
        StatusCode::UNAUTHORIZED => Error::Unauthorized,
        StatusCode::FORBIDDEN => Error::Forbidden(detail_of(resp).await),
        StatusCode::NOT_FOUND => Error::NotFound,
        _ => Error::Http {
            status: status.as_u16(),
            detail: detail_of(resp).await,
        },
    })
}
