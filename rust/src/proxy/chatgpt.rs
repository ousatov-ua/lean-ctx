use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::Response,
};

use super::{ProxyState, forward, openai_responses, openai_responses_ws};

/// Codex subscription model turns hit ChatGPT's Responses-compatible rail:
/// `/backend-api/codex/responses`. Forward through the same compressor/metering
/// path as OpenAI Responses, but target `https://chatgpt.com`.
pub async fn codex_responses_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let upstream = state.chatgpt_upstream();
    forward::forward_request(
        State(state),
        req,
        &upstream,
        "/backend-api/codex/responses",
        openai_responses::compress_request_body,
        "OpenAI",
        &[],
    )
    .await
}

/// Codex can use the Responses WebSocket transport in ChatGPT auth mode too.
pub async fn codex_responses_ws_handler(
    State(state): State<ProxyState>,
    headers: axum::http::HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    let upstream = state.chatgpt_upstream();
    openai_responses_ws::upgrade_to(
        state,
        ws,
        &headers,
        upstream,
        "/backend-api/codex/responses",
    )
}

/// ChatGPT aux calls (`/backend-api/wham/*`) are not model JSON and must not be
/// compressed or cost-metered. They are credential-preserving passthroughs.
pub async fn wham_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, forward::max_body_bytes())
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let upstream = state.chatgpt_upstream();
    let path = parts
        .uri
        .path_and_query()
        .map_or("/backend-api/wham", axum::http::uri::PathAndQuery::as_str);
    let url = format!("{upstream}{path}");

    let mut upstream_req = state.client.request(parts.method.clone(), &url);
    for (key, value) in &parts.headers {
        let k = key.as_str().to_lowercase();
        if forward::ALLOWED_REQUEST_HEADERS.contains(&k.as_str()) {
            upstream_req = upstream_req.header(key.clone(), value.clone());
        }
    }

    let response = upstream_req
        .body(body_bytes.to_vec())
        .send()
        .await
        .map_err(|e| {
            tracing::error!("lean-ctx proxy: ChatGPT WHAM upstream error: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let headers = response.headers().clone();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut out = Response::builder().status(status);
    for (key, value) in &headers {
        let k = key.as_str().to_lowercase();
        if forward::FORWARDED_HEADERS.contains(&k.as_str()) {
            out = out.header(key, value);
        }
    }
    out.body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
