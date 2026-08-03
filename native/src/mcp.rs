use anyhow::Result;
use rmcp::{
    ErrorData as McpError,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars::JsonSchema,
    tool, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::broker::BrowserRequestHandle;

#[derive(Clone)]
pub(crate) struct EffectorMcp {
    browser: BrowserRequestHandle,
}

impl EffectorMcp {
    pub(crate) fn new(browser: BrowserRequestHandle) -> Self {
        Self { browser }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TabsListParams {
    /// Only return tabs in this Chrome window.
    window_id: Option<i32>,
    /// Only return tabs in this Chrome tab group. Use -1 for ungrouped tabs.
    group_id: Option<i32>,
    /// Only return active tabs.
    active_only: Option<bool>,
    /// Only return discarded tabs without waking them.
    discarded_only: Option<bool>,
    /// Maximum tabs in this page. Defaults to 100 and is capped at 250.
    limit: Option<usize>,
    /// Zero-based offset returned as nextCursor by the preceding page.
    cursor: Option<usize>,
}

#[tool_router(server_handler)]
impl EffectorMcp {
    #[tool(
        name = "browser.list",
        description = "List the connected real Chrome browser instance and inventory counts"
    )]
    async fn browser_list(&self) -> Result<CallToolResult, McpError> {
        Ok(self.forward("browser.list", json!({})).await)
    }

    #[tool(
        name = "tabs.list",
        description = "List real Chrome windows, tab groups, and a bounded page of tab metadata without activating discarded tabs"
    )]
    async fn tabs_list(
        &self,
        Parameters(params): Parameters<TabsListParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = serde_json::to_value(params).unwrap_or_else(|_| json!({}));
        Ok(self.forward("tabs.list", params).await)
    }

    async fn forward(&self, method: &str, params: Value) -> CallToolResult {
        match self.browser.request(method, params).await {
            Ok(result) => {
                let text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let mut response = CallToolResult::success(vec![ContentBlock::text(text)]);
                response.structured_content = Some(result);
                response
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Effector browser request failed: {error:#}"
            ))]),
        }
    }
}
