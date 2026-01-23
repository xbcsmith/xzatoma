use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzatoma::config::CopilotConfig;
use xzatoma::providers::CopilotProvider;

/// Basic 401 -> non-interactive refresh -> retry flow for Copilot models
#[tokio::test]
async fn test_copilot_models_401_refresh_retry() {
    let server = MockServer::start().await;

    // Create provider config that points at the mock server
    let cfg = CopilotConfig {
        api_base: Some(server.uri()),
        ..Default::default()
    };

    let provider = CopilotProvider::new(cfg).unwrap();

    // Seed keyring with an (expiring) copilot token and a valid GitHub token
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cached = json!({
        "github_token": "gho_old",
        "copilot_token": "initial_token",
        "expires_at": now + 3600
    });

    let entry = keyring::Entry::new("xzatoma", "github_copilot").unwrap();
    entry.set_password(&cached.to_string()).unwrap();

    // First models request with the initial token should return 401
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer initial_token"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1)
        .mount(&server)
        .await;

    // Token exchange should be called with the GitHub token and return a new Copilot token
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .and(header("authorization", "token gho_old"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "new_token"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Retry models request should succeed with the refreshed token
    let models_body = json!({
        "data": [{
            "id": "gpt-5-mini",
            "name": "gpt-5-mini",
            "capabilities": {
                "limits": { "max_context_window_tokens": 264000 },
                "supports": { "tool_calls": true, "vision": false }
            },
            "policy": { "state": "enabled" }
        }]
    });

    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer new_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body))
        .expect(1)
        .mount(&server)
        .await;

    // Call list_models which should perform refresh and return models
    let models =
        <xzatoma::providers::CopilotProvider as xzatoma::providers::Provider>::list_models(
            &provider,
        )
        .await
        .unwrap();
    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.name == "gpt-5-mini"));
}

/// Verify models list is cached and second call does not hit the server
#[tokio::test]
async fn test_copilot_models_caching_ttl() {
    let server = MockServer::start().await;

    let cfg = CopilotConfig {
        api_base: Some(server.uri()),
        ..Default::default()
    };

    let provider = CopilotProvider::new(cfg).unwrap();

    // Seed keyring with a valid Copilot token so authenticate() uses it
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cached = json!({
        "github_token": "gho_dummy",
        "copilot_token": "initial_token",
        "expires_at": now + 3600
    });
    let entry = keyring::Entry::new("xzatoma", "github_copilot").unwrap();
    entry.set_password(&cached.to_string()).unwrap();

    // Only one models request is expected even if list_models is called twice
    let models_body = json!({
        "data": [{
            "id": "gpt-5-mini",
            "name": "gpt-5-mini",
            "capabilities": {
                "limits": { "max_context_window_tokens": 264000 },
                "supports": { "tool_calls": true, "vision": false }
            },
            "policy": { "state": "enabled" }
        }]
    });

    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer initial_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body))
        .expect(1)
        .mount(&server)
        .await;

    // First call hits the server
    let models1 =
        <xzatoma::providers::CopilotProvider as xzatoma::providers::Provider>::list_models(
            &provider,
        )
        .await
        .unwrap();
    assert!(!models1.is_empty());

    // Second call should use cache and not trigger another request (expectation = 1)
    let models2 =
        <xzatoma::providers::CopilotProvider as xzatoma::providers::Provider>::list_models(
            &provider,
        )
        .await
        .unwrap();
    assert_eq!(models1.len(), models2.len());
}
