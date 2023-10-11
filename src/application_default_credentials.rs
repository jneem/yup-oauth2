use crate::error::Error;
use crate::types::TokenInfo;
use http::Uri;
use hyper::client::connect::Connection;
use std::error::Error as StdError;
use tokio::io::{AsyncRead, AsyncWrite};
use tower_service::Service;

/// Provide options for the Application Default Credential Flow, mostly used for testing
#[derive(Default, Clone, Debug)]
pub struct ApplicationDefaultCredentialsFlowOpts {
    /// Used as base to build the url during token request from GCP metadata server
    pub metadata_url: Option<String>,
    /// If true, asks for an ID token instead of an OAuth access token.
    pub id_token: bool,
}

pub struct ApplicationDefaultCredentialsFlow {
    metadata_url: String,
    id_token: bool,
}

impl ApplicationDefaultCredentialsFlow {
    pub(crate) fn new(opts: ApplicationDefaultCredentialsFlowOpts) -> Self {
        let id_token = opts.id_token;
        let metadata_url = opts.metadata_url
            .unwrap_or_else(|| if id_token {
                 "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token".to_string()
            } else {
                 "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity".to_string()
            });
        ApplicationDefaultCredentialsFlow {
            metadata_url,
            id_token,
        }
    }

    pub(crate) async fn token<S, T>(
        &self,
        hyper_client: &hyper::Client<S>,
        scopes: &[T],
    ) -> Result<TokenInfo, Error>
    where
        T: AsRef<str>,
        S: Service<Uri> + Clone + Send + Sync + 'static,
        S::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
        S::Future: Send + Unpin + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let scope = crate::helper::join(scopes, ",");
        let token_uri = format!(
            "{}?{}={}",
            self.metadata_url,
            // For ID tokens we use the scope arguments to pass the audience. Bit of a hack, since
            // this was originally designed only for the access tokens.
            if self.id_token { "audience" } else { "scopes" },
            scope
        );
        let request = hyper::Request::get(token_uri)
            .header("Metadata-Flavor", "Google")
            .body(hyper::Body::from(String::new())) // why body is needed?
            .unwrap();
        log::debug!("requesting token from metadata server: {:?}", request);
        let (head, body) = hyper_client.request(request).await?.into_parts();
        let body = hyper::body::to_bytes(body).await?;
        log::debug!("received response; head: {:?}, body: {:?}", head, body);
        TokenInfo::from_json(&body)
    }
}

// eof
