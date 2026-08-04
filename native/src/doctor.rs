use anyhow::{Context, Result, bail};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientInfo},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Map, Value};

use crate::settings;

pub async fn run() -> Result<()> {
    let settings = settings::load_mcp_settings()?;
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(settings.endpoint())
            .auth_header(settings.token),
    );
    let client = ClientInfo::default()
        .serve(transport)
        .await
        .context("connect to Effector MCP endpoint; start Chrome with the extension installed")?;
    let result = client
        .call_tool(
            CallToolRequestParams::new("browser.snapshot").with_arguments(Map::from_iter([(
                "detail".to_owned(),
                Value::String("counts".to_owned()),
            )])),
        )
        .await
        .context("call browser.snapshot")?;
    let _ = client.cancel().await;

    if result.is_error == Some(true) {
        bail!("Effector MCP is reachable, but the Chrome request failed");
    }
    println!("Effector MCP broker and Chrome extension are reachable.");
    Ok(())
}
