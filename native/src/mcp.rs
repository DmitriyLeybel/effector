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

use crate::{
    broker::BrowserRequestHandle,
    browser_snapshot::{
        BrowserSnapshotParams, BrowserSnapshotToolOutput, decode_params,
        execute as execute_browser_snapshot,
    },
    protocol::{BrowserMethod, DomainError},
    runtime::BrokerRuntime,
};

#[derive(Clone)]
pub(crate) struct EffectorMcp {
    browser: BrowserRequestHandle,
    runtime: BrokerRuntime,
}

impl EffectorMcp {
    pub(crate) fn new(browser: BrowserRequestHandle, runtime: BrokerRuntime) -> Self {
        Self { browser, runtime }
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
        Ok(self.forward(BrowserMethod::BrowserList, json!({})).await)
    }

    #[tool(
        name = "browser.snapshot",
        description = "Return a compact, stable page of the connected Chrome window, Tab Group, and tab hierarchy without activating or waking tabs",
        input_schema = rmcp::handler::server::common::schema_for_input::<BrowserSnapshotParams>()
            .unwrap_or_else(|error| panic!("invalid browser.snapshot input schema: {error}")),
        output_schema = rmcp::handler::server::tool::schema_for_type::<BrowserSnapshotToolOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn browser_snapshot(
        &self,
        Parameters(params): Parameters<Value>,
    ) -> Result<CallToolResult, McpError> {
        let params = match decode_params(params) {
            Ok(params) => params,
            Err(error) => {
                return Ok(snapshot_result(
                    BrowserSnapshotToolOutput::Error(error),
                    true,
                ));
            }
        };
        let output = execute_browser_snapshot(&self.runtime, &self.browser, params).await;
        Ok(match output {
            Ok(output) => snapshot_result(output, false),
            Err(error) => snapshot_result(BrowserSnapshotToolOutput::Error(error), true),
        })
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
        Ok(self.forward(BrowserMethod::TabsList, params).await)
    }

    async fn forward(&self, method: BrowserMethod, params: Value) -> CallToolResult {
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

fn snapshot_result(output: BrowserSnapshotToolOutput, is_error: bool) -> CallToolResult {
    let summary = output.summary();
    let structured = match serde_json::to_value(&output) {
        Ok(structured) => structured,
        Err(_) => {
            let fallback = BrowserSnapshotToolOutput::Error(DomainError::new(
                "INTERNAL_ERROR",
                "The browser snapshot result could not be serialized.",
            ));
            return snapshot_result(fallback, true);
        }
    };
    let mut result = if is_error {
        CallToolResult::error(vec![ContentBlock::text(summary)])
    } else {
        CallToolResult::success(vec![ContentBlock::text(summary)])
    };
    result.structured_content = Some(structured);
    result
}
