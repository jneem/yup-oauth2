use yup_oauth2::{
    authenticator::Authenticator,
    authenticator_delegate::{DeviceAuthResponse, DeviceFlowDelegate, InstalledFlowDelegate},
    error::{AuthError, AuthErrorCode},
    ApplicationSecret, DeviceFlowAuthenticator, Error, InstalledFlowAuthenticator,
    InstalledFlowReturnMethod, ServiceAccountAuthenticator, ServiceAccountKey,
};

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use hyper::client::connect::HttpConnector;
use hyper::Uri;
use hyper_rustls::HttpsConnector;
use url::form_urlencoded;

/// Utility function for parsing json. Useful in unit tests. Simply wrap the
/// json! macro in a from_value to deserialize the contents to arbitrary structs.
macro_rules! parse_json {
    ($($json:tt)+) => {
        ::serde_json::from_value(::serde_json::json!($($json)+)).expect("failed to deserialize")
    }
}

async fn create_device_flow_auth() -> Authenticator<HttpsConnector<HttpConnector>> {
    let server_url = mockito::server_url();
    let app_secret: ApplicationSecret = parse_json!({
        "client_id": "902216714886-k2v9uei3p1dk6h686jbsn9mo96tnbvto.apps.googleusercontent.com",
        "project_id": "yup-test-243420",
        "auth_uri": "https://accounts.google.com/o/oauth2/auth",
        "token_uri": format!("{}/token", server_url),
        "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
        "client_secret": "iuMPN6Ne1PD7cos29Tk9rlqH",
        "redirect_uris": ["urn:ietf:wg:oauth:2.0:oob","http://localhost"],
    });
    struct FD;
    impl DeviceFlowDelegate for FD {
        fn present_user_code<'a>(
            &'a self,
            pi: &'a DeviceAuthResponse,
        ) -> Pin<Box<dyn Future<Output = ()> + 'a + Send>> {
            assert_eq!("https://example.com/verify", pi.verification_uri);
            Box::pin(futures::future::ready(()))
        }
    }

    DeviceFlowAuthenticator::builder(app_secret)
        .flow_delegate(Box::new(FD))
        .device_code_url(format!("{}/code", server_url))
        .build()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_device_success() {
    let auth = create_device_flow_auth().await;
    let code_response = serde_json::json!({
        "device_code": "devicecode",
        "user_code": "usercode",
        "verification_url": "https://example.com/verify",
        "expires_in": 1234567,
        "interval": 1
    });
    let _m = mockito::mock("POST", "/code")
        .match_body(mockito::Matcher::Regex(
            ".*client_id=902216714886.*".to_string(),
        ))
        .with_status(200)
        .with_body(code_response.to_string())
        .create();
    let token_response = serde_json::json!({
        "access_token": "accesstoken",
        "refresh_token": "refreshtoken",
        "token_type": "Bearer",
        "expires_in": 1234567
    });
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*client_secret=iuMPN6Ne1PD7cos29Tk9rlqH&code=devicecode.*".to_string(),
        ))
        .with_status(200)
        .with_body(token_response.to_string())
        .create();

    let token = auth
        .token(&["https://www.googleapis.com/scope/1"])
        .await
        .expect("token failed");
    assert_eq!("accesstoken", token.access_token);
    _m.assert();
}

#[tokio::test]
async fn test_device_no_code() {
    let auth = create_device_flow_auth().await;
    let code_response = serde_json::json!({
        "error": "invalid_client_id",
        "error_description": "description"
    });
    let _m = mockito::mock("POST", "/code")
        .match_body(mockito::Matcher::Regex(
            ".*client_id=902216714886.*".to_string(),
        ))
        .with_status(400)
        .with_body(code_response.to_string())
        .create();
    let token_response = serde_json::json!({
        "access_token": "accesstoken",
        "refresh_token": "refreshtoken",
        "token_type": "Bearer",
        "expires_in": 1234567
    });
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*client_secret=iuMPN6Ne1PD7cos29Tk9rlqH&code=devicecode.*".to_string(),
        ))
        .with_status(200)
        .with_body(token_response.to_string())
        .expect(0) // Never called!
        .create();

    let res = auth.token(&["https://www.googleapis.com/scope/1"]).await;
    assert!(res.is_err());
    assert!(format!("{}", res.unwrap_err()).contains("invalid_client_id"));
    _m.assert();
}

#[tokio::test]
async fn test_device_no_token() {
    let auth = create_device_flow_auth().await;
    let code_response = serde_json::json!({
        "device_code": "devicecode",
        "user_code": "usercode",
        "verification_url": "https://example.com/verify",
        "expires_in": 1234567,
        "interval": 1
    });
    let _m = mockito::mock("POST", "/code")
        .match_body(mockito::Matcher::Regex(
            ".*client_id=902216714886.*".to_string(),
        ))
        .with_status(200)
        .with_body(code_response.to_string())
        .create();
    let token_response = serde_json::json!({"error": "access_denied"});
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*client_secret=iuMPN6Ne1PD7cos29Tk9rlqH&code=devicecode.*".to_string(),
        ))
        .with_status(400)
        .with_body(token_response.to_string())
        .expect(1)
        .create();

    let res = auth.token(&["https://www.googleapis.com/scope/1"]).await;
    assert!(res.is_err());
    assert!(format!("{}", res.unwrap_err()).contains("access_denied"));
    _m.assert();
}

async fn create_installed_flow_auth(
    method: InstalledFlowReturnMethod,
    filename: Option<PathBuf>,
) -> Authenticator<HttpsConnector<HttpConnector>> {
    let server_url = mockito::server_url();
    let app_secret: ApplicationSecret = parse_json!({
        "client_id": "902216714886-k2v9uei3p1dk6h686jbsn9mo96tnbvto.apps.googleusercontent.com",
        "project_id": "yup-test-243420",
        "auth_uri": "https://accounts.google.com/o/oauth2/auth",
        "token_uri": format!("{}/token", server_url),
        "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
        "client_secret": "iuMPN6Ne1PD7cos29Tk9rlqH",
        "redirect_uris": ["urn:ietf:wg:oauth:2.0:oob","http://localhost"],
    });
    struct FD(hyper::Client<HttpsConnector<HttpConnector>>);
    impl InstalledFlowDelegate for FD {
        /// Depending on need_code, return the pre-set code or send the code to the server at
        /// the redirect_uri given in the url.
        fn present_user_url<'a>(
            &'a self,
            url: &'a str,
            need_code: bool,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            use std::str::FromStr;
            Box::pin(async move {
                if need_code {
                    Ok("authorizationcode".to_owned())
                } else {
                    // Parse presented url to obtain redirect_uri with location of local
                    // code-accepting server.
                    let uri = Uri::from_str(url.as_ref()).unwrap();
                    let query = uri.query().unwrap();
                    let parsed = form_urlencoded::parse(query.as_bytes()).into_owned();
                    let mut rduri = None;
                    for (k, v) in parsed {
                        if k == "redirect_uri" {
                            rduri = Some(v);
                            break;
                        }
                    }
                    if rduri.is_none() {
                        return Err("no redirect_uri!".into());
                    }
                    let mut rduri = rduri.unwrap();
                    rduri.push_str("?code=authorizationcode");
                    let rduri = Uri::from_str(rduri.as_ref()).unwrap();
                    // Hit server.
                    self.0
                        .get(rduri)
                        .await
                        .map_err(|e| e.to_string())
                        .map(|_| "".to_string())
                }
            })
        }
    }

    let mut builder = InstalledFlowAuthenticator::builder(app_secret, method).flow_delegate(
        Box::new(FD(hyper::Client::builder().build(HttpsConnector::new()))),
    );

    builder = if let Some(filename) = filename {
        builder.persist_tokens_to_disk(filename)
    } else {
        builder
    };

    builder.build().await.unwrap()
}

#[tokio::test]
async fn test_installed_interactive_success() {
    let auth = create_installed_flow_auth(InstalledFlowReturnMethod::Interactive, None).await;
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken",
                "refresh_token": "refreshtoken",
                "token_type": "Bearer",
                "expires_in": 12345678
            })
            .to_string(),
        )
        .expect(1)
        .create();

    let tok = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!("accesstoken", tok.access_token);
    assert_eq!("refreshtoken", tok.refresh_token.unwrap());
    assert_eq!("Bearer", tok.token_type);
    _m.assert();
}

#[tokio::test]
async fn test_installed_redirect_success() {
    let auth = create_installed_flow_auth(InstalledFlowReturnMethod::HTTPRedirect, None).await;
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken",
                "refresh_token": "refreshtoken",
                "token_type": "Bearer",
                "expires_in": 12345678
            })
            .to_string(),
        )
        .expect(1)
        .create();

    let tok = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!("accesstoken", tok.access_token);
    assert_eq!("refreshtoken", tok.refresh_token.unwrap());
    assert_eq!("Bearer", tok.token_type);
    _m.assert();
}

#[tokio::test]
async fn test_installed_error() {
    let auth = create_installed_flow_auth(InstalledFlowReturnMethod::Interactive, None).await;
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_status(400)
        .with_body(serde_json::json!({"error": "invalid_code"}).to_string())
        .expect(1)
        .create();

    let tokr = auth.token(&["https://googleapis.com/some/scope"]).await;
    assert!(tokr.is_err());
    assert!(format!("{}", tokr.unwrap_err()).contains("invalid_code"));
    _m.assert();
}

async fn create_service_account_auth() -> Authenticator<HttpsConnector<HttpConnector>> {
    let server_url = &mockito::server_url();
    let key: ServiceAccountKey = parse_json!({
        "type": "service_account",
        "project_id": "yup-test-243420",
        "private_key_id": "26de294916614a5ebdf7a065307ed3ea9941902b",
        "private_key": "-----BEGIN PRIVATE KEY-----\nMIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQDemmylrvp1KcOn\n9yTAVVKPpnpYznvBvcAU8Qjwr2fSKylpn7FQI54wCk5VJVom0jHpAmhxDmNiP8yv\nHaqsef+87Oc0n1yZ71/IbeRcHZc2OBB33/LCFqf272kThyJo3qspEqhuAw0e8neg\nLQb4jpm9PsqR8IjOoAtXQSu3j0zkXemMYFy93PWHjVpPEUX16NGfsWH7oxspBHOk\n9JPGJL8VJdbiAoDSDgF0y9RjJY5I52UeHNhMsAkTYs6mIG4kKXt2+T9tAyHw8aho\nwmuytQAfydTflTfTG8abRtliF3nil2taAc5VB07dP1b4dVYy/9r6M8Z0z4XM7aP+\nNdn2TKm3AgMBAAECggEAWi54nqTlXcr2M5l535uRb5Xz0f+Q/pv3ceR2iT+ekXQf\n+mUSShOr9e1u76rKu5iDVNE/a7H3DGopa7ZamzZvp2PYhSacttZV2RbAIZtxU6th\n7JajPAM+t9klGh6wj4jKEcE30B3XVnbHhPJI9TCcUyFZoscuPXt0LLy/z8Uz0v4B\nd5JARwyxDMb53VXwukQ8nNY2jP7WtUig6zwE5lWBPFMbi8GwGkeGZOruAK5sPPwY\nGBAlfofKANI7xKx9UXhRwisB4+/XI1L0Q6xJySv9P+IAhDUI6z6kxR+WkyT/YpG3\nX9gSZJc7qEaxTIuDjtep9GTaoEqiGntjaFBRKoe+VQKBgQDzM1+Ii+REQqrGlUJo\nx7KiVNAIY/zggu866VyziU6h5wjpsoW+2Npv6Dv7nWvsvFodrwe50Y3IzKtquIal\nVd8aa50E72JNImtK/o5Nx6xK0VySjHX6cyKENxHRDnBmNfbALRM+vbD9zMD0lz2q\nmns/RwRGq3/98EqxP+nHgHSr9QKBgQDqUYsFAAfvfT4I75Glc9svRv8IsaemOm07\nW1LCwPnj1MWOhsTxpNF23YmCBupZGZPSBFQobgmHVjQ3AIo6I2ioV6A+G2Xq/JCF\nmzfbvZfqtbbd+nVgF9Jr1Ic5T4thQhAvDHGUN77BpjEqZCQLAnUWJx9x7e2xvuBl\n1A6XDwH/ewKBgQDv4hVyNyIR3nxaYjFd7tQZYHTOQenVffEAd9wzTtVbxuo4sRlR\nNM7JIRXBSvaATQzKSLHjLHqgvJi8LITLIlds1QbNLl4U3UVddJbiy3f7WGTqPFfG\nkLhUF4mgXpCpkMLxrcRU14Bz5vnQiDmQRM4ajS7/kfwue00BZpxuZxst3QKBgQCI\nRI3FhaQXyc0m4zPfdYYVc4NjqfVmfXoC1/REYHey4I1XetbT9Nb/+ow6ew0UbgSC\nUZQjwwJ1m1NYXU8FyovVwsfk9ogJ5YGiwYb1msfbbnv/keVq0c/Ed9+AG9th30qM\nIf93hAfClITpMz2mzXIMRQpLdmQSR4A2l+E4RjkSOwKBgQCB78AyIdIHSkDAnCxz\nupJjhxEhtQ88uoADxRoEga7H/2OFmmPsqfytU4+TWIdal4K+nBCBWRvAX1cU47vH\nJOlSOZI0gRKe0O4bRBQc8GXJn/ubhYSxI02IgkdGrIKpOb5GG10m85ZvqsXw3bKn\nRVHMD0ObF5iORjZUqD0yRitAdg==\n-----END PRIVATE KEY-----\n",
        "client_email": "yup-test-sa-1@yup-test-243420.iam.gserviceaccount.com",
        "client_id": "102851967901799660408",
        "auth_uri": "https://accounts.google.com/o/oauth2/auth",
        "token_uri": format!("{}/token", server_url),
        "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
        "client_x509_cert_url": "https://www.googleapis.com/robot/v1/metadata/x509/yup-test-sa-1%40yup-test-243420.iam.gserviceaccount.com"
    });

    ServiceAccountAuthenticator::builder(key)
        .build()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_service_account_success() {
    use chrono::Utc;
    let auth = create_service_account_auth().await;

    let json_response = serde_json::json!({
        "access_token": "ya29.c.ElouBywiys0LyNaZoLPJcp1Fdi2KjFMxzvYKLXkTdvM-rDfqKlvEq6PiMhGoGHx97t5FAvz3eb_ahdwlBjSStxHtDVQB4ZPRJQ_EOi-iS7PnayahU2S9Jp8S6rk",
        "expires_in": 3600,
        "token_type": "Bearer"
    });
    let _m = mockito::mock("POST", "/token")
        .with_status(200)
        .with_header("content-type", "text/json")
        .with_body(json_response.to_string())
        .expect(1)
        .create();
    let tok = auth
        .token(&["https://www.googleapis.com/auth/pubsub"])
        .await
        .expect("token failed");
    assert!(tok.access_token.contains("ya29.c.ElouBywiys0Ly"));
    assert!(Utc::now() + chrono::Duration::seconds(3600) >= tok.expires_at.unwrap());
    _m.assert();
}

#[tokio::test]
async fn test_service_account_error() {
    let auth = create_service_account_auth().await;
    let bad_json_response = serde_json::json!({
        "error": "access_denied",
    });

    let _m = mockito::mock("POST", "/token")
        .with_status(200)
        .with_header("content-type", "text/json")
        .with_body(bad_json_response.to_string())
        .create();
    let result = auth
        .token(&["https://www.googleapis.com/auth/pubsub"])
        .await;
    assert!(result.is_err());
    _m.assert();
}

#[tokio::test]
async fn test_refresh() {
    let auth = create_installed_flow_auth(InstalledFlowReturnMethod::Interactive, None).await;
    // We refresh a token whenever it's within 1 minute of expiring. So
    // acquiring a token that expires in 59 seconds will force a refresh on
    // the next token call.
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken",
                "refresh_token": "refreshtoken",
                "token_type": "Bearer",
                "expires_in": 59,
            })
            .to_string(),
        )
        .expect(1)
        .create();
    let tok = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!("accesstoken", tok.access_token);
    assert_eq!("refreshtoken", tok.refresh_token.unwrap());
    assert_eq!("Bearer", tok.token_type);
    _m.assert();

    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*client_id=9022167.*refresh_token=refreshtoken.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken2",
                "token_type": "Bearer",
                "expires_in": 59,
            })
            .to_string(),
        )
        .expect(1)
        .create();

    let tok = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!("accesstoken2", tok.access_token);
    assert_eq!("refreshtoken", tok.refresh_token.unwrap());
    assert_eq!("Bearer", tok.token_type);
    _m.assert();

    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*client_id=9022167.*refresh_token=refreshtoken.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "error": "invalid_request",
            })
            .to_string(),
        )
        .expect(1)
        .create();

    let tok_err = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect_err("token refresh succeeded unexpectedly");
    match tok_err {
        Error::AuthError(AuthError {
            error: AuthErrorCode::InvalidRequest,
            ..
        }) => {}
        e => panic!("unexpected error on refresh: {:?}", e),
    }
    _m.assert();
}

#[tokio::test]
async fn test_memory_storage() {
    let auth = create_installed_flow_auth(InstalledFlowReturnMethod::Interactive, None).await;
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken",
                "refresh_token": "refreshtoken",
                "token_type": "Bearer",
                "expires_in": 12345678
            })
            .to_string(),
        )
        .expect(1)
        .create();

    // Call token twice. Ensure that only one http request is made and
    // identical tokens are returned.
    let token1 = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    let token2 = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!(token1.access_token.as_str(), "accesstoken");
    assert_eq!(token1, token2);
    _m.assert();

    // Create a new authenticator. This authenticator does not share a cache
    // with the previous one. Validate that it receives a different token.
    let auth2 = create_installed_flow_auth(InstalledFlowReturnMethod::Interactive, None).await;
    let _m = mockito::mock("POST", "/token")
        .match_body(mockito::Matcher::Regex(
            ".*code=authorizationcode.*client_id=9022167.*".to_string(),
        ))
        .with_body(
            serde_json::json!({
                "access_token": "accesstoken2",
                "refresh_token": "refreshtoken2",
                "token_type": "Bearer",
                "expires_in": 12345678
            })
            .to_string(),
        )
        .expect(1)
        .create();
    let token3 = auth2
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!(token3.access_token.as_str(), "accesstoken2");
    _m.assert();
}

#[tokio::test]
async fn test_disk_storage() {
    let tempdir = tempfile::tempdir().unwrap();
    let storage_path = tempdir.path().join("tokenstorage.json");
    {
        let auth = create_installed_flow_auth(
            InstalledFlowReturnMethod::Interactive,
            Some(storage_path.clone()),
        )
        .await;
        let _m = mockito::mock("POST", "/token")
            .match_body(mockito::Matcher::Regex(
                ".*code=authorizationcode.*client_id=9022167.*".to_string(),
            ))
            .with_body(
                serde_json::json!({
                    "access_token": "accesstoken",
                    "refresh_token": "refreshtoken",
                    "token_type": "Bearer",
                    "expires_in": 12345678
                })
                .to_string(),
            )
            .expect(1)
            .create();

        // Call token twice. Ensure that only one http request is made and
        // identical tokens are returned.
        let token1 = auth
            .token(&["https://googleapis.com/some/scope"])
            .await
            .expect("failed to get token");
        let token2 = auth
            .token(&["https://googleapis.com/some/scope"])
            .await
            .expect("failed to get token");
        assert_eq!(token1.access_token.as_str(), "accesstoken");
        assert_eq!(token1, token2);
        _m.assert();
    }

    // Create a new authenticator. This authenticator uses the same token
    // storage file as the previous one so should receive a token without
    // making any http requests.
    let auth = create_installed_flow_auth(
        InstalledFlowReturnMethod::Interactive,
        Some(storage_path.clone()),
    )
    .await;
    // Call token twice. Ensure that identical tokens are returned.
    let token1 = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    let token2 = auth
        .token(&["https://googleapis.com/some/scope"])
        .await
        .expect("failed to get token");
    assert_eq!(token1.access_token.as_str(), "accesstoken");
    assert_eq!(token1, token2);
}