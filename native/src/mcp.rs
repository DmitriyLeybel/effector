use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ResultType, ServerCapabilities, ServerInfo,
        SubscriptionFilter,
    },
    schemars::JsonSchema,
    service::{NotificationContext, RequestContext, RoleServer, SubscriptionContext},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};

use crate::{
    broker::BrowserRequestHandle,
    browser_snapshot::{
        BrowserSnapshotParams, BrowserSnapshotToolOutput, decode_params,
        execute as execute_browser_snapshot,
    },
    capabilities::ToolListNotifier,
    protocol::{BrowserMethod, DomainError},
    runtime::BrokerRuntime,
};

#[derive(Clone)]
pub(crate) struct EffectorMcp {
    browser: BrowserRequestHandle,
    runtime: BrokerRuntime,
    legacy_tool_list_notifier: std::sync::Arc<ToolListNotifier>,
    subscription_notifiers: std::sync::Arc<Mutex<VecDeque<std::sync::Arc<ToolListNotifier>>>>,
}

impl EffectorMcp {
    pub(crate) fn new(browser: BrowserRequestHandle, runtime: BrokerRuntime) -> Self {
        Self {
            browser,
            runtime,
            legacy_tool_list_notifier: ToolListNotifier::new(),
            subscription_notifiers: std::sync::Arc::new(Mutex::new(VecDeque::new())),
        }
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

#[tool_router]
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

#[tool_handler]
impl ServerHandler for EffectorMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let discovery = self.runtime.discovery_snapshot();
        let tools = Self::tool_router()
            .list_all()
            .into_iter()
            .filter(|tool| discovery.allows(tool.name.as_ref()))
            .collect();
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: None,
            cache_scope: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !self
            .runtime
            .discovery_snapshot()
            .allows(request.name.as_ref())
        {
            return Err(McpError::invalid_params("tool not found", None));
        }
        let router = Self::tool_router();
        router
            .call(ToolCallContext::new(self, request, context))
            .await
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        self.runtime
            .register_tool_list_notifier(&self.legacy_tool_list_notifier);
        self.legacy_tool_list_notifier.start_legacy(context.peer);
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        if requested.tools_list_changed == Some(true) {
            let notifier = ToolListNotifier::new();
            self.runtime.register_tool_list_notifier(&notifier);
            self.subscription_notifiers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_back(notifier);
        }
        Some(requested.supported_by(&self.get_info().capabilities))
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        if context.accepted().tools_list_changed != Some(true) {
            context.cancelled().await;
            return Ok(());
        }
        let notifier = self
            .subscription_notifiers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front();
        let Some(notifier) = notifier else {
            return Err(McpError::internal_error(
                "tool-list subscription state was unavailable",
                None,
            ));
        };
        notifier.listen(context).await;
        Ok(())
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
