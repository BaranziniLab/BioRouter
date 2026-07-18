use axum::{
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};

const MCP_UI_PROXY_HTML: &str = include_str!("templates/mcp_ui_proxy.html");

#[utoipa::path(
    get,
    path = "/mcp-ui-proxy",
    responses(
        (status = 200, description = "MCP UI proxy HTML page", content_type = "text/html"),
    )
)]
async fn mcp_ui_proxy() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::HeaderName::from_static("referrer-policy"),
                "no-referrer",
            ),
        ],
        Html(MCP_UI_PROXY_HTML),
    )
        .into_response()
}

pub fn routes() -> Router {
    Router::new().route("/mcp-ui-proxy", get(mcp_ui_proxy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn proxy_document_does_not_require_a_secret_in_its_url() {
        assert_eq!(mcp_ui_proxy().await.status(), StatusCode::OK);
        assert!(!MCP_UI_PROXY_HTML.contains("payload.sandbox"));
        assert!(!MCP_UI_PROXY_HTML.contains("params.get('url')"));
        assert!(MCP_UI_PROXY_HTML.contains("sandbox', 'allow-scripts allow-downloads'"));
    }
}
