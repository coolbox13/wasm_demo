use spin_sdk::http::{IntoResponse, Request, Response};
use spin_sdk::http_component;

// This little macro turns our Rust function into a Wasm component
// that can handle an HTTP request. Simple.
#[http_component]
async fn handle_request(req: Request) -> anyhow::Result<impl IntoResponse> {
    // Notice what's NOT here? No web server setup, no OS boilerplate.
    // Just the code that actually matters.
    let who = req.header("user-agent")
        .and_then(|h| h.as_str())
        .unwrap_or("Human");
    Ok(Response::builder()
        .status(200)
        .header("content-type", "text/plain")
        .body(format!("Hello, {}! Your request was handled by a tiny, lightning-fast Wasm component.", who))
        .build())
}
