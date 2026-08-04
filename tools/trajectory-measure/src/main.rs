use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tiktoken_rs::{CoreBPE, o200k_base};

const CORPUS_JSON: &str = include_str!("../corpus-v1.json");
const EXPECTED_SCHEMA_REVISION: &str = "trajectory-measure/v2";
const EXPECTED_CORPUS_REVISION: &str = "p3-0-synthetic-v2";
const FIXED_PROMPT: &str =
    "# Synthetic workflow\nEstablish the requested final state and verify the result.\n";
const CHECK_COMMAND: &str =
    "cargo run --locked --manifest-path tools/trajectory-measure/Cargo.toml -- check";
const TOKENIZER_OPERATION: &str = "tiktoken_rs::o200k_base()?.count_ordinary(rendered)";
const BREAK_EVEN_SEARCH_BOUND: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Corpus {
    schema_revision: String,
    corpus_revision: String,
    status: String,
    command: String,
    tokenizer: TokenizerSpec,
    scope_lock: ScopeLock,
    workflow_classes: Vec<String>,
    discovery: Discovery,
    trajectories: Vec<Trajectory>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScopeLock {
    target_tools: Vec<String>,
    new_permission: bool,
    new_trust_boundary: bool,
    new_policy_layer: bool,
    implementation_authority: String,
    confirm_destructive: String,
    automatic_activation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TokenizerSpec {
    crate_name: String,
    crate_version: String,
    encoding: String,
    mode: String,
    operation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Discovery {
    profiles: BTreeMap<String, Vec<String>>,
    common: BTreeMap<String, Value>,
    page_act: PageActDefinitions,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageActDefinitions {
    singular: Value,
    bounded: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Trajectory {
    id: String,
    workflow_class: String,
    weight: u32,
    kind: String,
    #[serde(default)]
    recovery_of: Option<String>,
    expected_terminal: Value,
    alternatives: BTreeMap<String, Alternative>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Alternative {
    steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Step {
    id: String,
    tool: String,
    arguments: Value,
    structured_content: Value,
    summary: String,
    #[serde(default)]
    is_error: bool,
    dispatch: DispatchAttribution,
    #[serde(default)]
    artifact: Option<Artifact>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum DispatchAttribution {
    Extension,
    BrokerLocal,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    decoded_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct Metrics {
    calls: usize,
    dispatches: usize,
    request_bytes: usize,
    response_bytes: usize,
    native_payload_bytes: usize,
    native_framed_bytes: usize,
    structured_bytes: usize,
    summary_bytes: usize,
    final_tokens: usize,
    aggregate_turn_tokens: usize,
    artifacts: usize,
    artifact_encoded_bytes: usize,
    artifact_decoded_bytes: usize,
}

impl std::ops::AddAssign for Metrics {
    fn add_assign(&mut self, rhs: Self) {
        self.calls += rhs.calls;
        self.dispatches += rhs.dispatches;
        self.request_bytes += rhs.request_bytes;
        self.response_bytes += rhs.response_bytes;
        self.native_payload_bytes += rhs.native_payload_bytes;
        self.native_framed_bytes += rhs.native_framed_bytes;
        self.structured_bytes += rhs.structured_bytes;
        self.summary_bytes += rhs.summary_bytes;
        self.final_tokens += rhs.final_tokens;
        self.aggregate_turn_tokens += rhs.aggregate_turn_tokens;
        self.artifacts += rhs.artifacts;
        self.artifact_encoded_bytes += rhs.artifact_encoded_bytes;
        self.artifact_decoded_bytes += rhs.artifact_decoded_bytes;
    }
}

#[derive(Debug)]
struct Classification {
    eligible: bool,
    actions: Vec<Value>,
}

#[derive(Debug)]
struct GateResults {
    eligible_success_round_trips: bool,
    eligible_success_tokens: bool,
    common_with_recovery: bool,
    aggregate_percent: bool,
    aggregate_absolute: bool,
    ordinary_growth: bool,
    discovery_growth: bool,
    recovery_no_cost: bool,
    singular_eligible: Metrics,
    bounded_eligible: Metrics,
    eligible_count: usize,
    eligible_success_count: usize,
    recovery_count: usize,
    ordinary_singular_tokens: usize,
    ordinary_bounded_tokens: usize,
    discovery_singular_tokens: usize,
    discovery_bounded_tokens: usize,
}

impl GateResults {
    fn passed(&self) -> bool {
        self.eligible_success_round_trips
            && self.eligible_success_tokens
            && self.common_with_recovery
            && self.aggregate_percent
            && self.aggregate_absolute
            && self.ordinary_growth
            && self.discovery_growth
            && self.recovery_no_cost
    }
}

fn main() -> Result<()> {
    let command = env::args().nth(1).unwrap_or_else(|| "check".to_owned());
    ensure!(
        command == "check" || command == "write",
        "usage: trajectory-measure [check|write]"
    );

    let corpus: Corpus = serde_json::from_str(CORPUS_JSON).context("parse corpus-v1.json")?;
    let tokenizer = o200k_base().context("initialize o200k_base")?;
    validate_corpus(&corpus)?;
    let report = generate_report(&corpus, &tokenizer)?;
    validate_report_privacy(&report)?;
    let report_path = report_path();

    if command == "write" {
        fs::write(&report_path, report)
            .with_context(|| format!("write {}", report_path.display()))?;
        println!("wrote {}", report_path.display());
        return Ok(());
    }

    let checked_in = fs::read_to_string(&report_path)
        .with_context(|| format!("read {}; run `cargo run -- write`", report_path.display()))?;
    ensure!(
        checked_in.as_bytes() == report.as_bytes(),
        "{} is stale; run `cargo run -- write`",
        report_path.display()
    );
    println!("{} is current", report_path.display());
    Ok(())
}

fn report_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/measurements/p3-0-v1.md")
}

fn validate_corpus(corpus: &Corpus) -> Result<()> {
    ensure!(
        corpus.schema_revision == EXPECTED_SCHEMA_REVISION,
        "unexpected schema revision"
    );
    ensure!(
        corpus.corpus_revision == EXPECTED_CORPUS_REVISION,
        "unexpected corpus revision"
    );
    ensure!(
        corpus.status == "proposed-non-executable",
        "corpus must remain proposed and non-executable"
    );
    ensure!(
        corpus.command == CHECK_COMMAND,
        "unexpected executable command"
    );
    ensure!(
        corpus.tokenizer.crate_name == "tiktoken-rs",
        "unexpected tokenizer crate"
    );
    ensure!(
        corpus.tokenizer.crate_version == "0.12.0",
        "unexpected tokenizer version"
    );
    ensure!(
        corpus.tokenizer.encoding == "o200k_base",
        "unexpected tokenizer encoding"
    );
    ensure!(
        corpus.tokenizer.mode == "ordinary",
        "unexpected tokenizer mode"
    );
    ensure!(
        corpus.tokenizer.operation == TOKENIZER_OPERATION,
        "unexpected tokenizer operation"
    );
    validate_scope_lock(&corpus.scope_lock)?;

    let expected_classes = BTreeSet::from([
        "stable-form".to_owned(),
        "off-viewport".to_owned(),
        "spa-settling".to_owned(),
        "active-visual".to_owned(),
        "inactive-visual-recovery".to_owned(),
        "browser-organization".to_owned(),
    ]);
    ensure!(
        corpus
            .workflow_classes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            == expected_classes,
        "workflow classes do not match G3.1"
    );

    validate_discovery(&corpus.discovery)?;
    ensure!(
        corpus.trajectories.len() >= 9,
        "at least nine trajectories are required"
    );
    let ids: BTreeSet<_> = corpus
        .trajectories
        .iter()
        .map(|trajectory| trajectory.id.as_str())
        .collect();
    ensure!(
        ids.len() == corpus.trajectories.len(),
        "trajectory IDs must be unique"
    );
    for required in [
        "stable_form_success",
        "stable_form_rerender_stale_partial_recovery",
        "off_viewport_scroll_click_success",
        "off_viewport_scroll_click_timeout_unknown_recovery",
        "spa_delayed_polling",
        "spa_capability_revoked",
        "active_scroll_visual_inspect_after",
        "inactive_visual_structured_recovery",
        "multi_window_group_destructive_preview_apply",
    ] {
        ensure!(
            ids.contains(required),
            "missing required trajectory {required}"
        );
    }

    let mut class_counts = BTreeMap::<&str, usize>::new();
    for trajectory in &corpus.trajectories {
        ensure!(
            trajectory.weight == 1,
            "every fixture must have equal unit weight"
        );
        ensure!(
            expected_classes.contains(&trajectory.workflow_class),
            "unknown workflow class"
        );
        *class_counts.entry(&trajectory.workflow_class).or_default() += 1;
        ensure!(
            trajectory.alternatives.len() == 2,
            "each fixture needs exactly two alternatives"
        );
        ensure!(
            trajectory.alternatives.contains_key("singular")
                && trajectory.alternatives.contains_key("bounded"),
            "missing alternative"
        );
        validate_steps(corpus, trajectory)?;
        validate_expected_terminal(trajectory)?;
        validate_snapshot_provenance(trajectory)?;
    }
    ensure!(
        class_counts
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            == 1,
        "workflow classes must have equal fixture counts"
    );

    for trajectory in &corpus.trajectories {
        match trajectory.kind.as_str() {
            "nominal" => {
                ensure!(
                    trajectory.recovery_of.is_none(),
                    "nominal fixture cannot inherit eligibility"
                );
                let singular = classify(corpus, trajectory, "singular")?;
                let bounded = classify(corpus, trajectory, "bounded")?;
                ensure!(
                    singular.eligible == bounded.eligible,
                    "alternatives classify differently"
                );
                ensure!(
                    singular.actions == bounded.actions,
                    "alternatives do not express the same actions"
                );
            }
            "recovery" => {
                let nominal_id = trajectory
                    .recovery_of
                    .as_deref()
                    .context("recovery fixture lacks nominal workflow")?;
                let nominal = corpus
                    .trajectories
                    .iter()
                    .find(|item| item.id == nominal_id)
                    .context("recovery nominal workflow not found")?;
                ensure!(nominal.kind == "nominal", "recovery parent must be nominal");
                let nominal_class = classify(corpus, nominal, "singular")?;
                ensure!(
                    nominal_class.eligible,
                    "recovery may inherit only mechanical eligibility"
                );
                validate_recovery_progress(trajectory, &nominal_class.actions)?;
            }
            _ => bail!("unknown trajectory kind"),
        }
    }
    validate_required_outcomes(corpus)?;
    Ok(())
}

fn validate_scope_lock(scope: &ScopeLock) -> Result<()> {
    ensure!(
        scope.target_tools
            == [
                "browser.snapshot",
                "browser.change",
                "page.inspect",
                "page.act",
                "page.evaluate",
            ],
        "G3.0 requires exactly five ordered target tools"
    );
    ensure!(
        !scope.new_permission && !scope.new_trust_boundary && !scope.new_policy_layer,
        "G3.0 forbids new permission, trust, or policy layers"
    );
    ensure!(
        scope.implementation_authority == "Part 2"
            && scope.confirm_destructive == "rejected"
            && scope.automatic_activation == "rejected",
        "G3.0 authority or rejection lock changed"
    );
    Ok(())
}

fn definitions(discovery: &Discovery, variant: &str) -> Result<BTreeMap<String, Value>> {
    let mut definitions = discovery.common.clone();
    let page_act = match variant {
        "singular" => &discovery.page_act.singular,
        "bounded" => &discovery.page_act.bounded,
        _ => bail!("unknown definition variant"),
    };
    definitions.insert("page.act".to_owned(), page_act.clone());
    Ok(definitions)
}

fn validate_discovery(discovery: &Discovery) -> Result<()> {
    let expected_profiles = BTreeMap::from([
        (
            "all-five",
            vec![
                "browser.snapshot",
                "browser.change",
                "page.inspect",
                "page.act",
                "page.evaluate",
            ],
        ),
        ("core", vec!["browser.snapshot", "browser.change"]),
        (
            "migration",
            vec![
                "browser.snapshot",
                "browser.change",
                "page.inspect",
                "page.act",
                "page.evaluate",
                "browser.list",
                "tabs.list",
            ],
        ),
        (
            "page",
            vec![
                "browser.snapshot",
                "browser.change",
                "page.inspect",
                "page.act",
            ],
        ),
    ]);
    for (profile, expected) in expected_profiles {
        let actual = discovery
            .profiles
            .get(profile)
            .with_context(|| format!("missing {profile} profile"))?;
        ensure!(
            actual.iter().map(String::as_str).collect::<Vec<_>>() == expected,
            "incorrect {profile} profile"
        );
    }
    ensure!(
        discovery.profiles.len() == 4,
        "unexpected discovery profile"
    );
    let singular = definitions(discovery, "singular")?;
    let bounded = definitions(discovery, "bounded")?;
    for catalog in [&singular, &bounded] {
        for (name, definition) in catalog {
            ensure!(
                definition.get("name").and_then(Value::as_str) == Some(name),
                "definition name mismatch for {name}"
            );
            for field in ["inputSchema", "outputSchema", "description", "annotations"] {
                ensure!(
                    definition.get(field).is_some(),
                    "incomplete {name} definition"
                );
            }
            compile_schema(&definition["inputSchema"], &format!("{name} input"))?;
            compile_schema(&definition["outputSchema"], &format!("{name} output"))?;
            ensure!(
                definition.get("xCardinality").is_none(),
                "harness-only discovery field"
            );
        }
        let inspect_defs = &catalog["page.inspect"]["outputSchema"]["$defs"];
        let action_defs = &catalog["page.act"]["outputSchema"]["$defs"];
        for shared in ["element", "semantic", "visual", "both"] {
            ensure!(
                inspect_defs[shared] == action_defs[shared],
                "page.act inspectAfter does not reuse page.inspect {shared}"
            );
        }
    }
    validate_definition_difference(&singular, &bounded)
}

fn validate_required_outcomes(corpus: &Corpus) -> Result<()> {
    let stale = trajectory(corpus, "stable_form_rerender_stale_partial_recovery")?;
    for variant in ["singular", "bounded"] {
        let steps = &stale.alternatives[variant].steps;
        ensure!(
            steps.iter().any(|step| step.tool == "page.inspect"
                && step.structured_content.pointer("/elements/0/ref")
                    == Some(&json!("el_terms_v2"))),
            "explicit replacement inspection missing"
        );
    }
    ensure!(
        stale.alternatives["singular"]
            .steps
            .iter()
            .any(|step| error_code(step) == Some("STALE_ELEMENT")),
        "singular stale outcome missing"
    );
    ensure!(
        stale.alternatives["bounded"].steps.iter().any(|step| step
            .structured_content
            .get("status")
            == Some(&json!("partial"))
            && step.structured_content.pointer("/error/code") == Some(&json!("STALE_ELEMENT"))),
        "bounded stale partial missing"
    );

    let timeout = trajectory(corpus, "off_viewport_scroll_click_timeout_unknown_recovery")?;
    for variant in ["singular", "bounded"] {
        ensure!(
            timeout.alternatives[variant].steps.iter().any(|step| step
                .structured_content
                .get("status")
                == Some(&json!("unknown"))
                && step.summary.to_ascii_lowercase().contains("timeout")),
            "timeout unknown missing"
        );
        ensure!(
            timeout.alternatives[variant]
                .steps
                .last()
                .is_some_and(|step| step.tool == "page.inspect" && !step.is_error),
            "unknown recovery lacks terminal inspection"
        );
    }

    let revoked = trajectory(corpus, "spa_capability_revoked")?;
    ensure!(
        revoked.alternatives["singular"]
            .steps
            .iter()
            .any(|step| error_code(step) == Some("CAPABILITY_DISABLED")),
        "singular revocation missing"
    );
    ensure!(
        revoked.alternatives["bounded"]
            .steps
            .iter()
            .any(|step| step.structured_content.pointer("/error/code")
                == Some(&json!("CAPABILITY_DISABLED"))),
        "bounded revocation missing"
    );

    let activation = trajectory(corpus, "inactive_visual_structured_recovery")?;
    for variant in ["singular", "bounded"] {
        let steps = &activation.alternatives[variant].steps;
        ensure!(
            error_code(&steps[0]) == Some("ACTIVATION_REQUIRED")
                && steps[0]
                    .structured_content
                    .pointer("/recovery/tabRef")
                    .is_some(),
            "activation recovery error missing"
        );
        ensure!(
            steps.len() == 5
                && steps[1].tool == "browser.snapshot"
                && is_broker_preview(&steps[2])
                && steps[3].tool == "browser.change"
                && steps[3].arguments.get("mode") == Some(&json!("apply"))
                && steps[4].tool == "page.inspect"
                && !steps[4].is_error,
            "activation snapshot-preview-apply-retry order changed"
        );
    }

    let destructive = trajectory(corpus, "multi_window_group_destructive_preview_apply")?;
    for variant in ["singular", "bounded"] {
        let preview = destructive.alternatives[variant]
            .steps
            .iter()
            .find(|step| is_broker_preview(step))
            .context("destructive preview missing")?;
        ensure!(
            preview.structured_content.get("destructive") == Some(&json!(true))
                && preview
                    .structured_content
                    .get("warnings")
                    .and_then(Value::as_array)
                    .is_some_and(|warnings| !warnings.is_empty()),
            "destructive metadata missing"
        );
    }
    Ok(())
}

fn trajectory<'a>(corpus: &'a Corpus, id: &str) -> Result<&'a Trajectory> {
    corpus
        .trajectories
        .iter()
        .find(|item| item.id == id)
        .with_context(|| format!("missing trajectory {id}"))
}

fn error_code(step: &Step) -> Option<&str> {
    step.is_error
        .then(|| step.structured_content.get("code")?.as_str())
        .flatten()
}

fn is_broker_preview(step: &Step) -> bool {
    step.tool == "browser.change"
        && step.arguments.get("mode") == Some(&json!("preview"))
        && step.dispatch == DispatchAttribution::BrokerLocal
}

fn validate_definition_difference(
    singular: &BTreeMap<String, Value>,
    bounded: &BTreeMap<String, Value>,
) -> Result<()> {
    ensure!(
        singular.keys().eq(bounded.keys()),
        "definition catalogs have different tools"
    );
    for (name, singular_definition) in singular {
        let bounded_definition = &bounded[name];
        if name != "page.act" {
            ensure!(
                singular_definition == bounded_definition,
                "unrelated definition differs for {name}"
            );
            continue;
        }
        let mut singular_common = singular_definition.clone();
        let mut bounded_common = bounded_definition.clone();
        for value in [&mut singular_common, &mut bounded_common] {
            let object = value
                .as_object_mut()
                .context("page.act definition must be an object")?;
            object.remove("inputSchema");
            object.remove("outputSchema");
        }
        ensure!(
            singular_common == bounded_common,
            "page.act differs outside cardinality, schema, or outcome fields"
        );

        let singular_input = &singular_definition["inputSchema"];
        let bounded_input = &bounded_definition["inputSchema"];
        ensure!(
            singular_input["$defs"] == bounded_input["$defs"],
            "page.act action definitions differ"
        );
        ensure!(
            singular_input["properties"]["action"]
                == bounded_input["properties"]["actions"]["items"]
                && bounded_input["properties"]["actions"]["minItems"] == json!(1)
                && bounded_input["properties"]["actions"]["maxItems"] == json!(3),
            "page.act cardinality schema is invalid"
        );
        let mut singular_input_common = singular_input.clone();
        let mut bounded_input_common = bounded_input.clone();
        for (schema, cardinality_field) in [
            (&mut singular_input_common, "action"),
            (&mut bounded_input_common, "actions"),
        ] {
            let object = schema
                .as_object_mut()
                .context("page.act input schema must be an object")?;
            let required = object
                .get_mut("required")
                .and_then(Value::as_array_mut)
                .context("page.act required fields must be an array")?;
            let cardinality = required
                .iter_mut()
                .find(|field| field.as_str() == Some(cardinality_field))
                .context("page.act cardinality field is not required")?;
            *cardinality = json!("cardinality");
            object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .context("page.act input properties must be an object")?
                .remove(cardinality_field)
                .context("page.act cardinality property is missing")?;
        }
        ensure!(
            singular_input_common == bounded_input_common,
            "page.act input differs outside cardinality"
        );

        let singular_output = &singular_definition["outputSchema"];
        let bounded_output = &bounded_definition["outputSchema"];
        ensure!(
            singular_output["$defs"] == bounded_output["$defs"],
            "page.act output definitions differ"
        );
        let singular_outcomes = singular_output["oneOf"]
            .as_array()
            .context("singular page.act outcomes must be an array")?;
        let bounded_outcomes = bounded_output["oneOf"]
            .as_array()
            .context("bounded page.act outcomes must be an array")?;
        ensure!(
            singular_outcomes.len() == 3
                && bounded_outcomes.len() == 4
                && singular_outcomes[0] == bounded_outcomes[0]
                && singular_outcomes[1] == bounded_outcomes[1],
            "page.act common success outcomes differ"
        );
        let mut bounded_unknown = bounded_outcomes[3].clone();
        let bounded_unknown_properties = bounded_unknown
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .context("bounded unknown properties must be an object")?;
        bounded_unknown_properties.remove("completedActions");
        bounded_unknown_properties.remove("uncertainActionIndex");
        bounded_unknown
            .as_object_mut()
            .context("bounded unknown outcome must be an object")?
            .remove("dependentRequired");
        ensure!(
            singular_outcomes[2] == bounded_unknown,
            "page.act common unknown outcome differs"
        );
        ensure!(
            bounded_outcomes[2]["properties"]["status"] == json!({"const":"partial"})
                && bounded_outcomes[2]["required"]
                    == json!([
                        "status",
                        "completedActions",
                        "failedActionIndex",
                        "error",
                        "documentRef"
                    ]),
            "bounded page.act partial outcome is invalid"
        );
        ensure!(
            singular_definition != bounded_definition,
            "page.act alternatives must differ"
        );
    }
    Ok(())
}

fn validate_steps(corpus: &Corpus, trajectory: &Trajectory) -> Result<()> {
    for (alternative_name, alternative) in &trajectory.alternatives {
        let catalog = definitions(&corpus.discovery, alternative_name)?;
        ensure!(
            !alternative.steps.is_empty(),
            "{} {alternative_name} has no calls",
            trajectory.id
        );
        for step in &alternative.steps {
            let definition = catalog
                .get(&step.tool)
                .with_context(|| format!("unknown tool {}", step.tool))?;
            validate_instance(
                &definition["inputSchema"],
                &step.arguments,
                &format!("{} {alternative_name} {} input", trajectory.id, step.id),
            )?;
            if step.is_error {
                validate_error(&step.structured_content)?;
            } else {
                validate_instance(
                    &definition["outputSchema"],
                    &step.structured_content,
                    &format!("{} {alternative_name} {} output", trajectory.id, step.id),
                )?;
            }
            let preview = step.tool == "browser.change"
                && step.arguments.get("mode") == Some(&json!("preview"));
            ensure!(
                preview == (step.dispatch == DispatchAttribution::BrokerLocal),
                "only browser.change preview may be broker-local"
            );
            if let Some(artifact) = &step.artifact {
                ensure!(
                    step.dispatch == DispatchAttribution::Extension,
                    "artifact must be extension-owned"
                );
                let data = artifact_data(artifact.decoded_bytes);
                let expected = 4 * artifact.decoded_bytes.div_ceil(3);
                ensure!(data.len() == expected, "base64 length formula mismatch");
                let expected_padding = match artifact.decoded_bytes % 3 {
                    0 => 0,
                    1 => 2,
                    2 => 1,
                    _ => unreachable!(),
                };
                ensure!(
                    data.ends_with(&"=".repeat(expected_padding))
                        && !data[..data.len() - expected_padding].contains('='),
                    "base64 padding mismatch"
                );
                ensure!(
                    BASE64
                        .decode(&data)
                        .context("decode generated artifact")?
                        .len()
                        == artifact.decoded_bytes,
                    "decoded artifact length mismatch"
                );
            }
            ensure!(
                contains_json_string(&step.structured_content, "image/png")
                    == step.artifact.is_some(),
                "visual result and image artifact must appear together"
            );
            let call = mcp_call(step);
            let result = mcp_result(step);
            ensure!(
                call.get("id") == result.get("id"),
                "generated MCP correlation mismatch"
            );
            if step.dispatch == DispatchAttribution::Extension {
                let request = native_request(step)?;
                let response = native_response(step);
                ensure!(
                    request.get("method") == Some(&json!(step.tool)),
                    "native request method does not correlate with MCP call"
                );
                ensure!(
                    request.get("requestId") == response.get("requestId"),
                    "native request IDs do not correlate"
                );
                if step.is_error {
                    ensure!(
                        response.get("ok") == Some(&json!(false))
                            && response.get("error").is_some(),
                        "native error response mismatch"
                    );
                } else {
                    let expected_result = if step.tool == "browser.snapshot" {
                        native_snapshot_baseline(&step.structured_content)
                    } else {
                        step.structured_content.clone()
                    };
                    ensure!(
                        response.get("ok") == Some(&json!(true))
                            && response.get("result") == Some(&expected_result),
                        "native result response mismatch"
                    );
                }
            }
        }
    }
    Ok(())
}

fn compile_schema(schema: &Value, label: &str) -> Result<()> {
    jsonschema::validator_for(schema)
        .map(|_| ())
        .with_context(|| format!("compile {label} schema"))
}

fn validate_instance(schema: &Value, instance: &Value, label: &str) -> Result<()> {
    let validator =
        jsonschema::validator_for(schema).with_context(|| format!("compile {label} schema"))?;
    validator
        .validate(instance)
        .map_err(|error| anyhow::anyhow!("{label}: {error}"))
}

fn validate_error(error: &Value) -> Result<()> {
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["code", "message"],
        "properties": {"code": {"type": "string", "minLength": 1}, "message": {"type": "string", "minLength": 1}, "recovery": {"type": "object", "additionalProperties": false, "required": ["tabRef"], "properties": {"tabRef": {"type": "string"}}}}
    });
    validate_instance(&schema, error, "domain error")?;
    ensure!(
        error.get("recovery").is_none() || error.get("code") == Some(&json!("ACTIVATION_REQUIRED")),
        "recovery is allowed only for ACTIVATION_REQUIRED"
    );
    Ok(())
}

fn contains_json_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(value) => value == expected,
        Value::Array(values) => values
            .iter()
            .any(|value| contains_json_string(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| contains_json_string(value, expected)),
        _ => false,
    }
}

fn validate_expected_terminal(trajectory: &Trajectory) -> Result<()> {
    for (variant, alternative) in &trajectory.alternatives {
        let terminal = alternative
            .steps
            .last()
            .context("trajectory has no terminal call")?;
        ensure!(!terminal.is_error, "terminal observation must succeed");
        ensure!(
            terminal.structured_content == trajectory.expected_terminal,
            "{} {variant} terminal structured content differs from expectedTerminal",
            trajectory.id
        );
    }
    Ok(())
}

fn collect_refs(value: &Value, refs: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("ref").and_then(Value::as_str) {
                refs.insert(reference.to_owned());
            }
            for child in object.values() {
                collect_refs(child, refs);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_refs(child, refs);
            }
        }
        _ => {}
    }
}

fn validate_snapshot_provenance(trajectory: &Trajectory) -> Result<()> {
    for alternative in trajectory.alternatives.values() {
        let mut snapshot_ref = None;
        let mut plan_ref = None;
        let mut refs = BTreeSet::new();
        for step in &alternative.steps {
            if step.tool == "browser.snapshot" && !step.is_error {
                snapshot_ref = step
                    .structured_content
                    .get("browserSnapshotRef")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                refs.clear();
                collect_refs(&step.structured_content["windows"], &mut refs);
            }
            if step.tool == "browser.change"
                && step.arguments.get("mode") == Some(&json!("preview"))
            {
                ensure!(
                    step.arguments
                        .get("browserSnapshotRef")
                        .and_then(Value::as_str)
                        == snapshot_ref.as_deref(),
                    "preview does not use preceding snapshot"
                );
                for operation in step.arguments["operations"]
                    .as_array()
                    .context("preview operations")?
                {
                    for (key, value) in operation.as_object().context("operation object")? {
                        if key.ends_with("Ref") {
                            ensure!(
                                refs.contains(value.as_str().context("reference string")?),
                                "operation invents unseen reference"
                            );
                        } else if key.ends_with("Refs") {
                            for reference in value.as_array().context("reference array")? {
                                ensure!(
                                    refs.contains(reference.as_str().context("reference string")?),
                                    "operation invents unseen reference"
                                );
                            }
                        }
                    }
                }
                plan_ref = step
                    .structured_content
                    .get("planRef")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            } else if step.tool == "browser.change"
                && step.arguments.get("mode") == Some(&json!("apply"))
            {
                ensure!(
                    step.arguments.get("planRef").and_then(Value::as_str) == plan_ref.as_deref(),
                    "apply does not use the preceding preview plan"
                );
            }
        }
    }
    Ok(())
}

fn classify(corpus: &Corpus, trajectory: &Trajectory, variant: &str) -> Result<Classification> {
    let alternative = &trajectory.alternatives[variant];
    let mut starting_refs = BTreeSet::new();
    let mut starting_document = None;
    let mut saw_start = false;
    let mut saw_action = false;
    let mut actions = Vec::new();
    let mut action_step_indexes = Vec::new();
    let definition = definitions(&corpus.discovery, variant)?["page.act"].clone();

    for (index, step) in alternative.steps.iter().enumerate() {
        if !saw_start
            && !saw_action
            && step.tool == "page.inspect"
            && !step.is_error
            && is_semantic_inspection(&step.arguments)
        {
            if let Some(elements) = step
                .structured_content
                .get("elements")
                .and_then(Value::as_array)
            {
                for element in elements {
                    if let Some(reference) = element.get("ref").and_then(Value::as_str) {
                        starting_refs.insert(reference.to_owned());
                    }
                }
            }
            starting_document = step
                .structured_content
                .get("documentRef")
                .and_then(Value::as_str)
                .map(str::to_owned);
            saw_start = true;
            continue;
        }
        if step.tool == "page.act" {
            saw_action = true;
            action_step_indexes.push(index);
            actions.extend(call_actions(&step.arguments)?);
        }
    }

    let mut eligible = saw_start && (2..=3).contains(&actions.len());
    for step in &alternative.steps {
        if step.tool == "page.act"
            && validate_instance(
                &definition["inputSchema"],
                &step.arguments,
                "classification action",
            )
            .is_err()
        {
            eligible = false;
        }
        if step.tool == "page.act" {
            eligible &= step.arguments.get("documentRef").and_then(Value::as_str)
                == starting_document.as_deref();
        }
    }
    for (index, action) in actions.iter().enumerate() {
        if let Some(reference) = action.get("elementRef").and_then(Value::as_str) {
            eligible &= starting_refs.contains(reference);
        } else {
            eligible &= is_allowed_reference_free_action(action);
        }
        if is_navigation_capable(action) && index + 1 != actions.len() {
            eligible = false;
        }
    }
    if let (Some(first), Some(last)) = (action_step_indexes.first(), action_step_indexes.last())
        && first < last
    {
        for step in &alternative.steps[first + 1..*last] {
            if step.tool == "page.inspect" {
                eligible = false;
            }
        }
    }
    if trajectory.kind == "nominal" {
        eligible &= alternative
            .steps
            .iter()
            .filter(|step| step.tool == "page.act")
            .all(|step| {
                !step.is_error
                    && !matches!(
                        step.structured_content
                            .get("status")
                            .and_then(Value::as_str),
                        Some("partial" | "unknown")
                    )
            });
        eligible &= alternative.steps.last().is_some_and(|step| {
            !step.is_error && step.structured_content == trajectory.expected_terminal
        });
    }
    Ok(Classification { eligible, actions })
}

fn is_semantic_inspection(arguments: &Value) -> bool {
    matches!(
        arguments.get("view").and_then(Value::as_str),
        None | Some("semantic")
    )
}

fn is_allowed_reference_free_action(action: &Value) -> bool {
    matches!(
        action.get("type").and_then(Value::as_str),
        Some("navigate" | "back" | "forward" | "reload")
    ) || (action.get("type") == Some(&json!("scroll"))
        && action.get("direction").is_some()
        && action.get("amount").is_some())
}

fn is_navigation_capable(action: &Value) -> bool {
    matches!(
        action.get("type").and_then(Value::as_str),
        Some("click" | "navigate" | "back" | "forward" | "reload")
    )
}

fn call_actions(arguments: &Value) -> Result<Vec<Value>> {
    if let Some(action) = arguments.get("action") {
        return Ok(vec![action.clone()]);
    }
    arguments
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .context("page.act lacks action cardinality")
}

fn validate_recovery_progress(trajectory: &Trajectory, nominal_actions: &[Value]) -> Result<()> {
    let nominal_signatures = nominal_actions
        .iter()
        .map(action_signature)
        .collect::<Result<Vec<_>>>()?;
    let mut final_progress = Vec::new();
    for (alternative_name, alternative) in &trajectory.alternatives {
        let mut progress = 0;
        let mut uncertain = false;
        let mut available_refs = BTreeSet::new();
        let mut replacement_refs = BTreeSet::new();
        let mut saw_replacement_inspection = false;
        for step in &alternative.steps {
            if step.tool == "page.inspect" && !step.is_error {
                if let Some(elements) = step
                    .structured_content
                    .get("elements")
                    .and_then(Value::as_array)
                {
                    let issued = elements
                        .iter()
                        .filter_map(|element| {
                            element
                                .get("ref")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                        .collect::<BTreeSet<_>>();
                    if available_refs.is_empty() {
                        available_refs.extend(issued);
                    } else {
                        replacement_refs = issued.clone();
                        available_refs.extend(issued);
                        saw_replacement_inspection = true;
                    }
                }
                continue;
            }
            if step.tool != "page.act" {
                continue;
            }
            ensure!(!uncertain, "recovery acts after an unknown outcome");
            let attempted = call_actions(&step.arguments)?;
            ensure!(
                progress + attempted.len() <= nominal_actions.len(),
                "recovery attempts too many actions"
            );
            for action in &attempted {
                if let Some(reference) = action.get("elementRef").and_then(Value::as_str) {
                    ensure!(
                        available_refs.contains(reference),
                        "{} {alternative_name} recovery action uses an element not returned by an earlier inspection",
                        trajectory.id
                    );
                    if trajectory.id == "stable_form_rerender_stale_partial_recovery"
                        && saw_replacement_inspection
                    {
                        ensure!(
                            replacement_refs.contains(reference),
                            "stale recovery reused an old reference"
                        );
                    }
                }
            }
            let attempted_signatures = attempted
                .iter()
                .map(action_signature)
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                attempted_signatures
                    == nominal_signatures[progress..progress + attempted_signatures.len()],
                "{} {alternative_name} recovery action order diverges from nominal at progress {progress}",
                trajectory.id
            );
            if step.is_error {
                continue;
            }
            match step
                .structured_content
                .get("status")
                .and_then(Value::as_str)
            {
                Some("partial") => {
                    let completed = step
                        .structured_content
                        .get("completedActions")
                        .and_then(Value::as_u64)
                        .context("partial result lacks progress")?
                        as usize;
                    ensure!(
                        completed < attempted.len(),
                        "partial result completed every attempted action"
                    );
                    ensure!(
                        step.structured_content
                            .get("failedActionIndex")
                            .and_then(Value::as_u64)
                            == Some(completed as u64),
                        "partial failure index is inconsistent"
                    );
                    progress += completed;
                }
                Some("unknown") => {
                    let completed = step
                        .structured_content
                        .get("completedActions")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    ensure!(
                        completed < attempted.len(),
                        "unknown result completed every attempted action"
                    );
                    if alternative_name == "bounded" {
                        ensure!(
                            step.structured_content
                                .get("uncertainActionIndex")
                                .and_then(Value::as_u64)
                                == Some(completed as u64),
                            "unknown action index is inconsistent with progress"
                        );
                    }
                    progress += completed;
                    uncertain = true;
                }
                _ => progress += attempted.len(),
            }
        }
        if trajectory.id == "stable_form_rerender_stale_partial_recovery" {
            ensure!(
                replacement_refs
                    == BTreeSet::from(["el_save_v2".to_owned(), "el_terms_v2".to_owned()]),
                "stale recovery replacement set changed"
            );
            let replacement = alternative
                .steps
                .iter()
                .find(|step| {
                    step.tool == "page.inspect"
                        && step.structured_content.pointer("/elements/0/ref")
                            == Some(&json!("el_terms_v2"))
                })
                .context("replacement inspection missing")?;
            ensure!(
                replacement.structured_content["elements"]
                    == json!([
                        {"ref":"el_terms_v2","role":"checkbox","name":"Replacement terms"},
                        {"ref":"el_save_v2","role":"button","name":"Replacement save"}
                    ]),
                "replacement control semantics changed"
            );
        }
        final_progress.push((progress, uncertain));
    }
    ensure!(
        final_progress.windows(2).all(|pair| pair[0] == pair[1]),
        "recovery alternatives report different action progress"
    );
    Ok(())
}

fn action_signature(action: &Value) -> Result<Value> {
    let mut signature = action.clone();
    signature
        .as_object_mut()
        .context("page action must be an object")?
        .remove("elementRef");
    Ok(signature)
}

fn canonical_json(value: &Value) -> Result<String> {
    serde_json::to_string(&canonical_value(value)).context("render canonical JSON")
}

fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            for key in keys {
                sorted.insert(key.clone(), canonical_value(&object[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_value).collect()),
        _ => value.clone(),
    }
}

fn canonical_model_json(value: &Value) -> Result<String> {
    serde_json::to_string(&canonical_model_value(value)).context("render canonical model JSON")
}

fn canonical_model_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            for key in keys {
                if key != "data" {
                    sorted.insert(key.clone(), canonical_model_value(&object[key]));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_model_value).collect()),
        _ => value.clone(),
    }
}

fn artifact_data(decoded_bytes: usize) -> String {
    let mut bytes = vec![0_u8; decoded_bytes];
    let signature = [137, 80, 78, 71, 13, 10, 26, 10];
    for (target, value) in bytes.iter_mut().zip(signature) {
        *target = value;
    }
    BASE64.encode(bytes)
}

fn mcp_call(step: &Step) -> Value {
    json!({"jsonrpc":"2.0","id":step.id,"method":"tools/call","params":{"name":step.tool,"arguments":step.arguments}})
}

fn mcp_result(step: &Step) -> Value {
    let mut content = vec![json!({"type":"text","text":step.summary})];
    if let Some(artifact) = &step.artifact {
        content.push(json!({"type":"image","mimeType":"image/png","data":artifact_data(artifact.decoded_bytes)}));
    }
    json!({"jsonrpc":"2.0","id":step.id,"result":{"content":content,"structuredContent":step.structured_content,"isError":step.is_error}})
}

fn request_policy(tool: &str) -> Result<(&'static str, u64)> {
    match tool {
        "browser.snapshot" | "browser.list" | "tabs.list" | "page.inspect" => Ok(("read", 29_000)),
        "browser.change" => Ok(("browserOperation", 58_000)),
        "page.act" => Ok(("pageAction", 43_000)),
        "page.evaluate" => Ok(("evaluation", 33_000)),
        _ => bail!("unknown native method {tool}"),
    }
}

fn native_request(step: &Step) -> Result<Value> {
    let (request_class, deadline_ms) = request_policy(&step.tool)?;
    let params = normalized_native_params(step)?;
    Ok(json!({
        "type":"request", "requestId":format!("native-{}", step.id), "method":step.tool,
        "params":params, "requestClass":request_class, "deadlineMs":deadline_ms
    }))
}

fn normalized_native_params(step: &Step) -> Result<Value> {
    if step.tool == "browser.snapshot" {
        return Ok(json!({}));
    }
    if step.tool == "browser.change" {
        ensure!(
            step.arguments.get("mode") == Some(&json!("apply")),
            "preview must remain broker-local"
        );
        return Ok(json!({
            "operationId": format!("operation-{}", step.id),
            "planKey": step.arguments.get("planRef").context("apply lacks planRef")?
        }));
    }
    Ok(internalize_references(&step.arguments))
}

fn internalize_references(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let key = match key.as_str() {
                        "tabRef" => "tabKey",
                        "documentRef" => "documentKey",
                        "elementRef" => "nodeKey",
                        _ => key,
                    };
                    (key.to_owned(), internalize_references(value))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(internalize_references).collect()),
        _ => value.clone(),
    }
}

fn native_response(step: &Step) -> Value {
    let dispatch_state = if step.is_error && step.tool == "page.act" {
        "notDispatched"
    } else {
        "completed"
    };
    let mut response = Map::from_iter([
        ("type".to_owned(), json!("response")),
        ("requestId".to_owned(), json!(format!("native-{}", step.id))),
        (
            "browserInstanceId".to_owned(),
            json!("browser_synthetic_v1"),
        ),
        ("ok".to_owned(), json!(!step.is_error)),
        ("dispatch".to_owned(), json!({"state":dispatch_state})),
    ]);
    if step.is_error {
        // The broker enriches ACTIVATION_REQUIRED with the tab reference known from routing.
        let mut error = step.structured_content.clone();
        error
            .as_object_mut()
            .expect("validated error object")
            .remove("recovery");
        response.insert("error".to_owned(), error);
    } else {
        let result = if step.tool == "browser.snapshot" {
            native_snapshot_baseline(&step.structured_content)
        } else {
            step.structured_content.clone()
        };
        response.insert("result".to_owned(), result);
    }
    if let Some(artifact) = &step.artifact {
        response.insert("artifacts".to_owned(), json!([{"type":"image","mimeType":"image/png","data":artifact_data(artifact.decoded_bytes)}]));
    }
    Value::Object(response)
}

fn native_snapshot_baseline(snapshot: &Value) -> Value {
    let mut windows = Vec::new();
    let mut groups = Vec::new();
    let mut tabs = Vec::new();
    let mut next_group_id = 1_i64;
    let mut next_tab_id = 1_i64;

    for (window_index, window) in snapshot["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let window_id = window_index as i64 + 1;
        let window_key = format!("window-key-{window_id}");
        let mut native_window = Map::from_iter([
            ("key".to_owned(), json!(window_key)),
            ("id".to_owned(), json!(window_id)),
            (
                "focused".to_owned(),
                json!(window.get("focused") == Some(&json!(true))),
            ),
        ]);
        for field in [
            "top",
            "left",
            "width",
            "height",
            "type",
            "state",
            "alwaysOnTop",
        ] {
            if let Some(value) = window.get(field) {
                native_window.insert(field.to_owned(), value.clone());
            }
        }
        windows.push(Value::Object(native_window));

        let mut tab_index = 0_i64;
        for item in window["items"].as_array().into_iter().flatten() {
            if let Some(group) = item.get("group") {
                let group_id = next_group_id;
                next_group_id += 1;
                let group_key = format!("group-key-{group_id}");
                let mut native_group = Map::from_iter([
                    ("key".to_owned(), json!(group_key)),
                    ("id".to_owned(), json!(group_id)),
                    ("windowKey".to_owned(), json!(window_key)),
                    ("color".to_owned(), group["color"].clone()),
                    (
                        "collapsed".to_owned(),
                        json!(
                            group
                                .get("collapsed")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        ),
                    ),
                ]);
                for field in ["title", "shared"] {
                    if let Some(value) = group.get(field) {
                        native_group.insert(field.to_owned(), value.clone());
                    }
                }
                groups.push(Value::Object(native_group));
                for tab in item["tabs"].as_array().into_iter().flatten() {
                    tabs.push(native_snapshot_tab(
                        tab,
                        next_tab_id,
                        &window_key,
                        tab_index,
                        Some(&group_key),
                    ));
                    next_tab_id += 1;
                    tab_index += 1;
                }
            } else {
                tabs.push(native_snapshot_tab(
                    item,
                    next_tab_id,
                    &window_key,
                    tab_index,
                    None,
                ));
                next_tab_id += 1;
                tab_index += 1;
            }
        }
    }

    json!({
        "browserInstanceId":"browser_synthetic_v1",
        "modelRevision":1,
        "capturedAt":snapshot.get("capturedAt").cloned().unwrap_or_else(|| json!("2030-01-01T00:00:00Z")),
        "supportsFrozenTabs":false,
        "supportsSharedTabGroups":false,
        "windows":windows,
        "groups":groups,
        "tabs":tabs
    })
}

fn native_snapshot_tab(
    tab: &Value,
    id: i64,
    window_key: &str,
    index: i64,
    group_key: Option<&str>,
) -> Value {
    let mut native = Map::from_iter([
        ("key".to_owned(), json!(format!("tab-key-{id}"))),
        ("id".to_owned(), json!(id)),
        ("windowKey".to_owned(), json!(window_key)),
        ("index".to_owned(), json!(index)),
        (
            "active".to_owned(),
            json!(tab.get("active") == Some(&json!(true))),
        ),
        ("highlighted".to_owned(), json!(false)),
        (
            "pinned".to_owned(),
            json!(tab.get("pinned") == Some(&json!(true))),
        ),
        (
            "discarded".to_owned(),
            json!(tab.get("discarded") == Some(&json!(true))),
        ),
    ]);
    if let Some(group_key) = group_key {
        native.insert("groupKey".to_owned(), json!(group_key));
    }
    for field in [
        "title",
        "url",
        "pendingUrl",
        "audible",
        "autoDiscardable",
        "lastAccessed",
        "favIconUrl",
    ] {
        if let Some(value) = tab.get(field) {
            native.insert(field.to_owned(), value.clone());
        }
    }
    Value::Object(native)
}

fn render_step(step: &Step) -> Result<String> {
    Ok(format!(
        "ASSISTANT_TOOL_CALL\n{}\nTOOL_RESULT_TEXT\n{}\nTOOL_RESULT_STRUCTURED\n{}\n",
        canonical_model_json(&json!({"name":step.tool,"arguments":step.arguments}))?,
        step.summary,
        canonical_model_json(&step.structured_content)?
    ))
}

fn render_trajectory_body(trajectory: &Trajectory, variant: &str) -> Result<String> {
    let mut rendered = format!("\n# Fixture\n{}\n{}", trajectory.id, FIXED_PROMPT);
    for step in &trajectory.alternatives[variant].steps {
        rendered.push_str(&render_step(step)?);
    }
    Ok(rendered)
}

fn measure_alternative(alternative: &Alternative, tokenizer: &CoreBPE) -> Result<Metrics> {
    let mut metrics = Metrics::default();
    let mut context = FIXED_PROMPT.to_owned();
    for step in &alternative.steps {
        let call = mcp_call(step);
        let result = mcp_result(step);
        metrics.calls += 1;
        metrics.request_bytes += canonical_json(&call)?.len();
        metrics.response_bytes += canonical_json(&result)?.len();
        if step.dispatch == DispatchAttribution::Extension {
            let request = native_request(step)?;
            let response = native_response(step);
            metrics.dispatches += 1;
            metrics.native_payload_bytes +=
                canonical_json(&request)?.len() + canonical_json(&response)?.len();
            metrics.native_framed_bytes +=
                canonical_json(&request)?.len() + canonical_json(&response)?.len() + 8;
        }
        metrics.structured_bytes += canonical_json(&step.structured_content)?.len();
        metrics.summary_bytes += step.summary.len();
        if let Some(artifact) = &step.artifact {
            let data = artifact_data(artifact.decoded_bytes);
            metrics.artifacts += 1;
            metrics.artifact_encoded_bytes += data.len();
            metrics.artifact_decoded_bytes += artifact.decoded_bytes;
        }
        context.push_str(&render_step(step)?);
        metrics.aggregate_turn_tokens += tokenizer.count_ordinary(&context);
    }
    metrics.final_tokens = tokenizer.count_ordinary(&context);
    Ok(metrics)
}

fn discovery_jsonrpc(discovery: &Discovery, variant: &str, profile: &str) -> Result<Value> {
    let definitions = definitions(discovery, variant)?;
    let names = discovery
        .profiles
        .get(profile)
        .context("unknown discovery profile")?;
    let tools = names
        .iter()
        .map(|name| definitions[name].clone())
        .collect::<Vec<_>>();
    Ok(json!({"jsonrpc":"2.0","id":"tools-list","result":{"tools":tools}}))
}

fn discovery_render(discovery: &Discovery, variant: &str, profile: &str) -> Result<String> {
    Ok(format!(
        "TOOLS\n{}\n",
        canonical_model_json(&discovery_jsonrpc(discovery, variant, profile)?)?
    ))
}

fn trajectory_is_eligible(corpus: &Corpus, trajectory: &Trajectory) -> Result<bool> {
    let nominal = if trajectory.kind == "nominal" {
        trajectory
    } else {
        let nominal_id = trajectory
            .recovery_of
            .as_deref()
            .context("recovery fixture lacks nominal workflow")?;
        corpus
            .trajectories
            .iter()
            .find(|item| item.id == nominal_id)
            .context("recovery nominal workflow not found")?
    };
    Ok(classify(corpus, nominal, "singular")?.eligible)
}

fn complete_eligible_context(corpus: &Corpus, variant: &str) -> Result<String> {
    let mut context = discovery_render(&corpus.discovery, variant, "all-five")?;
    for trajectory in &corpus.trajectories {
        if trajectory_is_eligible(corpus, trajectory)? {
            context.push_str(&render_trajectory_body(trajectory, variant)?);
        }
    }
    Ok(context)
}

fn trajectory_context_tokens(
    corpus: &Corpus,
    trajectory: &Trajectory,
    variant: &str,
    tokenizer: &CoreBPE,
) -> Result<usize> {
    let mut context = discovery_render(&corpus.discovery, variant, "all-five")?;
    context.push_str(&render_trajectory_body(trajectory, variant)?);
    Ok(tokenizer.count_ordinary(&context))
}

fn eligible_per_turn_tokens(corpus: &Corpus, variant: &str, tokenizer: &CoreBPE) -> Result<usize> {
    let discovery = discovery_render(&corpus.discovery, variant, "all-five")?;
    let mut total = 0;
    for trajectory in &corpus.trajectories {
        if !trajectory_is_eligible(corpus, trajectory)? {
            continue;
        }
        let mut visible = format!(
            "{}\n# Fixture\n{}\n{}",
            discovery, trajectory.id, FIXED_PROMPT
        );
        for step in &trajectory.alternatives[variant].steps {
            visible.push_str(&render_step(step)?);
            total += tokenizer.count_ordinary(&visible);
        }
    }
    Ok(total)
}

fn ordinary_trajectory(corpus: &Corpus) -> Result<&Trajectory> {
    corpus
        .trajectories
        .iter()
        .find(|trajectory| trajectory.id == "active_focus_visual_inspect_after")
        .context("ordinary one-action fixture not found")
}

fn ordinary_session_context(corpus: &Corpus, variant: &str, calls: usize) -> Result<String> {
    let ordinary = ordinary_trajectory(corpus)?;
    let body = render_trajectory_body(ordinary, variant)?;
    let mut context = discovery_render(&corpus.discovery, variant, "all-five")?;
    for _ in 0..calls {
        context.push_str(&body);
    }
    Ok(context)
}

fn ordinary_session_tokens(
    corpus: &Corpus,
    variant: &str,
    calls: usize,
    tokenizer: &CoreBPE,
) -> Result<usize> {
    Ok(tokenizer.count_ordinary(&ordinary_session_context(corpus, variant, calls)?))
}

fn eligible_workload_body(corpus: &Corpus, variant: &str) -> Result<String> {
    let mut body = String::new();
    for trajectory in &corpus.trajectories {
        if trajectory_is_eligible(corpus, trajectory)? {
            body.push_str(&render_trajectory_body(trajectory, variant)?);
        }
    }
    Ok(body)
}

fn candidate_session_tokens(
    corpus: &Corpus,
    variant: &str,
    repetitions: usize,
    tokenizer: &CoreBPE,
) -> Result<usize> {
    let mut context = discovery_render(&corpus.discovery, variant, "all-five")?;
    let body = eligible_workload_body(corpus, variant)?;
    for _ in 0..repetitions {
        context.push_str(&body);
    }
    Ok(tokenizer.count_ordinary(&context))
}

fn session_break_even(corpus: &Corpus, tokenizer: &CoreBPE) -> Result<Option<usize>> {
    let singular_calls = corpus
        .trajectories
        .iter()
        .filter_map(|trajectory| {
            trajectory_is_eligible(corpus, trajectory)
                .ok()
                .filter(|eligible| *eligible)
                .map(|_| trajectory.alternatives["singular"].steps.len())
        })
        .sum::<usize>();
    for repetitions in 1..=BREAK_EVEN_SEARCH_BOUND {
        let singular = candidate_session_tokens(corpus, "singular", repetitions, tokenizer)?;
        let bounded = candidate_session_tokens(corpus, "bounded", repetitions, tokenizer)?;
        if bounded <= singular {
            return Ok(Some(repetitions * singular_calls));
        }
    }
    Ok(None)
}

fn calculate_gates(corpus: &Corpus, tokenizer: &CoreBPE) -> Result<GateResults> {
    let mut singular_eligible = Metrics::default();
    let mut bounded_eligible = Metrics::default();
    let mut eligible_count = 0;
    let mut eligible_success_count = 0;
    let mut recovery_count = 0;
    let mut eligible_success_round_trips = true;
    let mut eligible_success_tokens = true;
    let mut recovery_no_cost = true;
    let mut common_nominals = BTreeSet::new();

    for trajectory in &corpus.trajectories {
        if !trajectory_is_eligible(corpus, trajectory)? {
            continue;
        }
        eligible_count += 1;
        let singular = measure_alternative(&trajectory.alternatives["singular"], tokenizer)?;
        let bounded = measure_alternative(&trajectory.alternatives["bounded"], tokenizer)?;
        singular_eligible += singular;
        bounded_eligible += bounded;
        if trajectory.kind == "nominal" {
            eligible_success_count += 1;
            eligible_success_round_trips &= singular.calls > bounded.calls;
            eligible_success_tokens &=
                trajectory_context_tokens(corpus, trajectory, "bounded", tokenizer)?
                    <= trajectory_context_tokens(corpus, trajectory, "singular", tokenizer)?;
        } else {
            recovery_count += 1;
            recovery_no_cost &=
                trajectory_context_tokens(corpus, trajectory, "bounded", tokenizer)?
                    <= trajectory_context_tokens(corpus, trajectory, "singular", tokenizer)?
                    && bounded.calls <= singular.calls;
            common_nominals.insert(trajectory.recovery_of.as_deref().unwrap_or_default());
        }
    }

    singular_eligible.final_tokens =
        tokenizer.count_ordinary(&complete_eligible_context(corpus, "singular")?);
    bounded_eligible.final_tokens =
        tokenizer.count_ordinary(&complete_eligible_context(corpus, "bounded")?);
    singular_eligible.aggregate_turn_tokens =
        eligible_per_turn_tokens(corpus, "singular", tokenizer)?;
    bounded_eligible.aggregate_turn_tokens =
        eligible_per_turn_tokens(corpus, "bounded", tokenizer)?;

    let common_with_recovery = eligible_success_count >= 2 && common_nominals.len() >= 2;
    let saved = singular_eligible
        .final_tokens
        .saturating_sub(bounded_eligible.final_tokens);
    let aggregate_percent = saved * 100 >= singular_eligible.final_tokens * 15;
    let aggregate_absolute = saved >= 250;

    let ordinary = ordinary_trajectory(corpus)?;
    let ordinary_singular_tokens =
        measure_alternative(&ordinary.alternatives["singular"], tokenizer)?.final_tokens;
    let ordinary_bounded_tokens =
        measure_alternative(&ordinary.alternatives["bounded"], tokenizer)?.final_tokens;
    let ordinary_growth = within_growth(ordinary_singular_tokens, ordinary_bounded_tokens, 50);

    let discovery_singular_tokens = tokenizer.count_ordinary(&discovery_render(
        &corpus.discovery,
        "singular",
        "all-five",
    )?);
    let discovery_bounded_tokens =
        tokenizer.count_ordinary(&discovery_render(&corpus.discovery, "bounded", "all-five")?);
    let discovery_growth = within_growth(discovery_singular_tokens, discovery_bounded_tokens, 100);

    Ok(GateResults {
        eligible_success_round_trips,
        eligible_success_tokens,
        common_with_recovery,
        aggregate_percent,
        aggregate_absolute,
        ordinary_growth,
        discovery_growth,
        recovery_no_cost,
        singular_eligible,
        bounded_eligible,
        eligible_count,
        eligible_success_count,
        recovery_count,
        ordinary_singular_tokens,
        ordinary_bounded_tokens,
        discovery_singular_tokens,
        discovery_bounded_tokens,
    })
}

fn within_growth(baseline: usize, candidate: usize, absolute_allowance: usize) -> bool {
    candidate <= baseline
        || candidate - baseline <= absolute_allowance
        || (candidate - baseline) * 100 <= baseline * 5
}

fn generate_report(corpus: &Corpus, tokenizer: &CoreBPE) -> Result<String> {
    let gates = calculate_gates(corpus, tokenizer)?;
    let numeric_passed = gates.passed();
    let sequence_decision = if numeric_passed {
        "NOT DECIDED"
    } else {
        "REJECTED FOR V1"
    };
    let mut output = String::new();
    output.push_str("# P3.0 synthetic measurement evidence v1\n\n");
    output.push_str("## Status\n\n");
    output.push_str("| Gate | Result |\n| --- | --- |\n");
    output.push_str("| P3.0 baseline and scope freeze | **PASS** |\n");
    output.push_str("| G3.0 scope lock | **PASS** |\n");
    output.push_str("| G3.1 workflow baseline | **PASS** |\n");
    output.push_str(&format!(
        "| G3.2 efficiency gate | **{}** |\n",
        pass_fail(numeric_passed)
    ));
    if numeric_passed {
        output.push_str("| G3.3 safety gate | **NOT RUN** |\n");
    } else {
        output
            .push_str("| G3.3 safety gate | **NOT RUN (unnecessary for rejected candidate)** |\n");
    }
    output.push_str(&format!(
        "| Sequence decision | **{sequence_decision}** |\n\n"
    ));
    if numeric_passed {
        output.push_str("The numeric precheck passes, but P2.6 implementation, G3.3 safety, named-client rendering, and live evidence remain absent, so no sequence decision or ADR is established.\n\n");
    } else {
        output.push_str("The mandatory efficiency gate fails, so the bounded sequence candidate is rejected for V1. Singular action remains; no sequence runtime or safety claim was established and no ADR is created. P2.6, named-client, and live evidence remain absent.\n\n");
    }
    output.push_str("All proposed production browser-change and page-tool branches measured here remain unadvertised.\n\n");

    output.push_str("## Method\n\n");
    output.push_str(&format!(
        "- Corpus revision: `{}`\n",
        corpus.corpus_revision
    ));
    output.push_str(&format!(
        "- Schema revision: `{}`\n",
        corpus.schema_revision
    ));
    output.push_str(&format!(
        "- Tokenizer: `{}` `{}`, encoding `{}`, mode `{}`\n",
        corpus.tokenizer.crate_name,
        corpus.tokenizer.crate_version,
        corpus.tokenizer.encoding,
        corpus.tokenizer.mode
    ));
    output.push_str(&format!("- Executable command: `{}`\n", corpus.command));
    output.push_str(&format!(
        "- Tokenizer operation: `{}`\n",
        corpus.tokenizer.operation
    ));
    output.push_str("- Canonical renderer: `TOOLS`, `ASSISTANT_TOOL_CALL`, `TOOL_RESULT_TEXT`, and `TOOL_RESULT_STRUCTURED`; recursively sorted compact JSON and preserved array order. This is not named-client evidence.\n");
    output.push_str("- Generated JSON-RPC envelopes are canonical. Protocol-v3 envelopes use normalized synthetic parameters for unimplemented branches and deterministic image base64; they are reproducible planning evidence, not shipped branch ABIs. Model rendering excludes image base64 and reports artifact bytes separately once.\n");
    output.push_str("- Every fixture has unit weight. Each of the six workflow classes has the same fixture count.\n");
    output.push_str("- Class-table token metrics are body-only. The eligible final context contains all-five discovery once followed by every eligible fixture in checked-in source order.\n");
    output.push_str("- Aggregate per-turn tokens replay each eligible fixture with all-five discovery and its fixed prompt in every visible prefix.\n");
    output.push_str(&format!("- Ordinary session totals are independently rendered and tokenized. Candidate break-even searches {BREAK_EVEN_SEARCH_BOUND} exact complete-workload repetitions.\n\n"));

    output.push_str("## Discovery Aggregates\n\n");
    output.push_str("| Profile | Singular bytes | Bounded bytes | Delta bytes | Singular tokens | Bounded tokens | Delta tokens |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in ["core", "page", "all-five", "migration"] {
        let singular_wire =
            canonical_json(&discovery_jsonrpc(&corpus.discovery, "singular", profile)?)?;
        let bounded_wire =
            canonical_json(&discovery_jsonrpc(&corpus.discovery, "bounded", profile)?)?;
        let singular_render = discovery_render(&corpus.discovery, "singular", profile)?;
        let bounded_render = discovery_render(&corpus.discovery, "bounded", profile)?;
        let singular_tokens = tokenizer.count_ordinary(&singular_render);
        let bounded_tokens = tokenizer.count_ordinary(&bounded_render);
        output.push_str(&format!(
            "| {} | {} | {} | {:+} | {} | {} | {:+} |\n",
            profile,
            singular_wire.len(),
            bounded_wire.len(),
            signed_delta(bounded_wire.len(), singular_wire.len()),
            singular_tokens,
            bounded_tokens,
            signed_delta(bounded_tokens, singular_tokens)
        ));
    }
    output.push('\n');

    output.push_str("## Workflow Aggregates\n\n");
    output.push_str("All rows combine both unit-weight fixtures in a class. Artifact bytes are accounted once and are not added to model tokens.\n\n");
    output.push_str("| Class | Shape | Calls | Native dispatches | JSON-RPC bytes | Native payload bytes | Native framed bytes | Structured bytes | Summary bytes | Body final tokens | Body per-turn tokens | Artifacts | Encoded artifact bytes | Decoded artifact bytes |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for class in &corpus.workflow_classes {
        for variant in ["singular", "bounded"] {
            let mut aggregate = Metrics::default();
            for trajectory in corpus
                .trajectories
                .iter()
                .filter(|trajectory| &trajectory.workflow_class == class)
            {
                aggregate += measure_alternative(&trajectory.alternatives[variant], tokenizer)?;
            }
            output.push_str(&metrics_row(class, variant, aggregate));
        }
    }
    output.push('\n');

    let token_delta = signed_delta(
        gates.bounded_eligible.final_tokens,
        gates.singular_eligible.final_tokens,
    );
    let saved = gates
        .singular_eligible
        .final_tokens
        .saturating_sub(gates.bounded_eligible.final_tokens);
    let percent = -(token_delta as f64) * 100.0 / gates.singular_eligible.final_tokens as f64;
    output.push_str("## G3.2 Numeric Projection\n\n");
    output.push_str(&format!("The mechanically eligible aggregate contains {} fixtures: {} nominal successes and {} recovery variants.\n\n", gates.eligible_count, gates.eligible_success_count, gates.recovery_count));
    output.push_str(
        "| Eligible aggregate | Singular | Bounded | Delta |\n| --- | ---: | ---: | ---: |\n",
    );
    output.push_str(&format!(
        "| MCP calls | {} | {} | {:+} |\n",
        gates.singular_eligible.calls,
        gates.bounded_eligible.calls,
        signed_delta(gates.bounded_eligible.calls, gates.singular_eligible.calls)
    ));
    output.push_str(&format!(
        "| Native dispatches | {} | {} | {:+} |\n",
        gates.singular_eligible.dispatches,
        gates.bounded_eligible.dispatches,
        signed_delta(
            gates.bounded_eligible.dispatches,
            gates.singular_eligible.dispatches
        )
    ));
    output.push_str(&format!(
        "| Final-context tokens | {} | {} | {:+} ({:.2}% savings) |\n",
        gates.singular_eligible.final_tokens,
        gates.bounded_eligible.final_tokens,
        token_delta,
        percent
    ));
    output.push_str(&format!(
        "| Aggregate per-turn tokens | {} | {} | {:+} |\n\n",
        gates.singular_eligible.aggregate_turn_tokens,
        gates.bounded_eligible.aggregate_turn_tokens,
        signed_delta(
            gates.bounded_eligible.aggregate_turn_tokens,
            gates.singular_eligible.aggregate_turn_tokens
        )
    ));

    output.push_str(
        "| Numeric precheck | Measured | Required | Result |\n| --- | ---: | ---: | --- |\n",
    );
    output.push_str(&format!(
        "| Every eligible success saves a round trip | {} successes | all | {} |\n",
        gates.eligible_success_count,
        pass_fail(gates.eligible_success_round_trips)
    ));
    output.push_str(&format!(
        "| Every eligible success has no token increase | {} successes | all | {} |\n",
        gates.eligible_success_count,
        pass_fail(gates.eligible_success_tokens)
    ));
    output.push_str(&format!(
        "| Common eligible workflows with recovery | {} workflows | at least 2 | {} |\n",
        gates.recovery_count,
        pass_fail(gates.common_with_recovery)
    ));
    output.push_str(&format!(
        "| Eligible aggregate relative savings | {:.2}% | at least 15% | {} |\n",
        percent,
        pass_fail(gates.aggregate_percent)
    ));
    output.push_str(&format!(
        "| Eligible aggregate absolute savings | {} tokens | at least 250 | {} |\n",
        saved,
        pass_fail(gates.aggregate_absolute)
    ));
    output.push_str(&format!(
        "| Ordinary one-action growth | {} to {} tokens ({:+}) | no more than max(5%, 50) | {} |\n",
        gates.ordinary_singular_tokens,
        gates.ordinary_bounded_tokens,
        signed_delta(
            gates.ordinary_bounded_tokens,
            gates.ordinary_singular_tokens
        ),
        pass_fail(gates.ordinary_growth)
    ));
    output.push_str(&format!(
        "| All-five discovery growth | {} to {} tokens ({:+}) | no more than max(5%, 100) | {} |\n",
        gates.discovery_singular_tokens,
        gates.discovery_bounded_tokens,
        signed_delta(
            gates.discovery_bounded_tokens,
            gates.discovery_singular_tokens
        ),
        pass_fail(gates.discovery_growth)
    ));
    output.push_str(&format!(
        "| Recovery cost | {} variants | bounded no greater than singular | {} |\n\n",
        gates.recovery_count,
        pass_fail(gates.recovery_no_cost)
    ));

    output.push_str("## Session Amortization\n\n");
    output.push_str("| Calls | Singular session tokens | Bounded session tokens | Singular amortized/call | Bounded amortized/call |\n| ---: | ---: | ---: | ---: | ---: |\n");
    for calls in [1, 5, 20] {
        let singular = ordinary_session_tokens(corpus, "singular", calls, tokenizer)?;
        let bounded = ordinary_session_tokens(corpus, "bounded", calls, tokenizer)?;
        output.push_str(&format!(
            "| {calls} | {} | {} | {:.2} | {:.2} |\n",
            singular,
            bounded,
            singular as f64 / calls as f64,
            bounded as f64 / calls as f64
        ));
    }
    output.push_str("\nCandidate break-even repeats the complete eligible-workload body behind one all-five discovery.\n");
    let singular_workload_calls = gates.singular_eligible.calls;
    match session_break_even(corpus, tokenizer)? {
        Some(calls) => output.push_str(&format!(
            "\nCandidate break-even: **{calls} singular MCP-call equivalents** by exact canonical tokenization.\n"
        )),
        None => output.push_str(&format!(
            "\nCandidate break-even: none within **{} singular MCP-call equivalents** ({} complete-workload repetitions).\n",
            BREAK_EVEN_SEARCH_BOUND * singular_workload_calls,
            BREAK_EVEN_SEARCH_BOUND
        )),
    }
    output.push_str(&format!(
        "\nThe sequence decision is **{sequence_decision}**.\n"
    ));
    Ok(output)
}

fn metrics_row(class: &str, variant: &str, metrics: Metrics) -> String {
    format!(
        "| {class} | {variant} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
        metrics.calls,
        metrics.dispatches,
        metrics.request_bytes + metrics.response_bytes,
        metrics.native_payload_bytes,
        metrics.native_framed_bytes,
        metrics.structured_bytes,
        metrics.summary_bytes,
        metrics.final_tokens,
        metrics.aggregate_turn_tokens,
        metrics.artifacts,
        metrics.artifact_encoded_bytes,
        metrics.artifact_decoded_bytes
    )
}

fn pass_fail(value: bool) -> &'static str {
    if value { "PASS" } else { "FAIL" }
}

fn signed_delta(candidate: usize, baseline: usize) -> isize {
    candidate as isize - baseline as isize
}

fn validate_report_privacy(report: &str) -> Result<()> {
    for forbidden in [
        "https://",
        "http://",
        "example.invalid",
        "doc_",
        "el_",
        "tab_",
        "win_",
        "grp_",
        "plan_",
        "bs_",
        "ps_",
        "base64...",
    ] {
        ensure!(
            !report.contains(forbidden),
            "generated report contains forbidden fixture material: {forbidden}"
        );
    }
    ensure!(
        !report
            .as_bytes()
            .windows(64)
            .any(|window| { window.iter().all(|byte| byte.is_ascii_hexdigit()) }),
        "generated report contains a likely 64-hex credential"
    );
    ensure!(
        !report
            .as_bytes()
            .windows(32)
            .any(|window| { window.iter().all(|byte| matches!(byte, b'a'..=b'p')) }),
        "generated report contains an extension-ID-shaped value"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_order_sorts_objects_and_preserves_arrays() {
        let value = json!({"z": 1, "a": {"d": 2, "b": 1}, "items": [3, 1, 2]});
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"a":{"b":1,"d":2},"items":[3,1,2],"z":1}"#
        );
        assert_eq!(
            canonical_model_json(&json!({"type": "image", "data": "excluded"})).unwrap(),
            r#"{"type":"image"}"#
        );
    }

    #[test]
    fn corpus_classification_is_mechanical() {
        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let stable = corpus
            .trajectories
            .iter()
            .find(|trajectory| trajectory.id == "stable_form_success")
            .unwrap();
        let visual = corpus
            .trajectories
            .iter()
            .find(|trajectory| trajectory.id == "active_focus_visual_inspect_after")
            .unwrap();
        assert!(classify(&corpus, stable, "singular").unwrap().eligible);
        assert!(!classify(&corpus, visual, "singular").unwrap().eligible);
    }

    #[test]
    fn unrelated_definition_difference_is_rejected() {
        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let singular = definitions(&corpus.discovery, "singular").unwrap();
        let mut bounded = definitions(&corpus.discovery, "bounded").unwrap();
        bounded.get_mut("page.inspect").unwrap()["description"] = json!("drift");
        assert!(validate_definition_difference(&singular, &bounded).is_err());
        let mut bounded = definitions(&corpus.discovery, "bounded").unwrap();
        bounded.get_mut("page.act").unwrap()["description"] = json!("unrelated drift");
        assert!(validate_definition_difference(&singular, &bounded).is_err());
        let mut bounded = definitions(&corpus.discovery, "bounded").unwrap();
        bounded.get_mut("page.act").unwrap()["inputSchema"]["$defs"]["click"]["properties"]["button"] =
            json!({"const":"secondary"});
        assert!(validate_definition_difference(&singular, &bounded).is_err());
        let mut bounded = definitions(&corpus.discovery, "bounded").unwrap();
        bounded.get_mut("page.act").unwrap()["inputSchema"]["required"] = json!(["actions"]);
        assert!(validate_definition_difference(&singular, &bounded).is_err());
        let mut bounded = definitions(&corpus.discovery, "bounded").unwrap();
        bounded.get_mut("page.act").unwrap()["outputSchema"]["oneOf"][3]["properties"]["unexpected"] =
            json!({"type":"boolean"});
        assert!(validate_definition_difference(&singular, &bounded).is_err());
    }

    #[test]
    fn browser_change_preview_has_zero_native_dispatches() {
        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let mut previews = 0;
        for trajectory in &corpus.trajectories {
            for alternative in trajectory.alternatives.values() {
                for step in &alternative.steps {
                    if step.tool == "browser.change"
                        && step.arguments.get("mode").and_then(Value::as_str) == Some("preview")
                    {
                        previews += 1;
                        assert_eq!(step.dispatch, DispatchAttribution::BrokerLocal);
                    }
                }
            }
        }
        assert_eq!(previews, 8);

        let organization = corpus
            .trajectories
            .iter()
            .find(|trajectory| trajectory.id == "multi_window_group_destructive_preview_apply")
            .unwrap();
        let tokenizer = o200k_base().unwrap();
        let metrics =
            measure_alternative(&organization.alternatives["singular"], &tokenizer).unwrap();
        assert_eq!(metrics.calls, 3);
        assert_eq!(metrics.dispatches, 2);

        let mut wrong_plan = organization.clone();
        wrong_plan.alternatives.get_mut("singular").unwrap().steps[2].arguments["planRef"] =
            json!("plan_other");
        assert!(validate_snapshot_provenance(&wrong_plan).is_err());
    }

    #[test]
    fn classifier_rejects_unsafe_or_malformed_nominals() {
        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let stable = trajectory(&corpus, "stable_form_success").unwrap();

        let mut after_action = stable.clone();
        after_action
            .alternatives
            .get_mut("singular")
            .unwrap()
            .steps
            .swap(0, 1);
        assert!(
            !classify(&corpus, &after_action, "singular")
                .unwrap()
                .eligible
        );

        let mut failed = stable.clone();
        failed.alternatives.get_mut("singular").unwrap().steps[1].is_error = true;
        failed.alternatives.get_mut("singular").unwrap().steps[1].structured_content =
            json!({"code":"NOT_ACTIONABLE","message":"Synthetic failure."});
        assert!(!classify(&corpus, &failed, "singular").unwrap().eligible);

        let mut unseen = stable.clone();
        unseen.alternatives.get_mut("singular").unwrap().steps[1].arguments["action"]["elementRef"] =
            json!("el_unseen");
        assert!(!classify(&corpus, &unseen, "singular").unwrap().eligible);

        let mut another_document = stable.clone();
        another_document
            .alternatives
            .get_mut("singular")
            .unwrap()
            .steps[2]
            .arguments["documentRef"] = json!("doc_other");
        assert!(
            !classify(&corpus, &another_document, "singular")
                .unwrap()
                .eligible
        );

        let mut nonterminal_click = stable.clone();
        let steps = &mut nonterminal_click
            .alternatives
            .get_mut("singular")
            .unwrap()
            .steps;
        let first = steps[1].arguments["action"].clone();
        steps[1].arguments["action"] = steps[3].arguments["action"].clone();
        steps[3].arguments["action"] = first;
        assert!(
            !classify(&corpus, &nonterminal_click, "singular")
                .unwrap()
                .eligible
        );

        let mut malformed = stable.clone();
        malformed.alternatives.get_mut("singular").unwrap().steps[1].arguments["action"]["unexpected"] =
            json!(true);
        assert!(!classify(&corpus, &malformed, "singular").unwrap().eligible);

        let mut missing_image = trajectory(&corpus, "active_scroll_visual_inspect_after")
            .unwrap()
            .clone();
        missing_image
            .alternatives
            .get_mut("singular")
            .unwrap()
            .steps[0]
            .artifact = None;
        assert!(validate_steps(&corpus, &missing_image).is_err());
    }

    #[test]
    fn concatenated_session_measurement_is_exact() {
        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let tokenizer = o200k_base().unwrap();
        let rendered = ordinary_session_context(&corpus, "singular", 5).unwrap();
        let exact = ordinary_session_tokens(&corpus, "singular", 5, &tokenizer).unwrap();
        assert_eq!(exact, tokenizer.count_ordinary(&rendered));

        let discovery = tokenizer
            .count_ordinary(&discovery_render(&corpus.discovery, "singular", "all-five").unwrap());
        let body = tokenizer.count_ordinary(
            &render_trajectory_body(ordinary_trajectory(&corpus).unwrap(), "singular").unwrap(),
        );
        assert_ne!(exact, discovery + body * 5);
    }

    #[test]
    fn gate_calculations_apply_exact_allowances() {
        assert!(within_growth(100, 150, 50));
        assert!(!within_growth(100, 151, 50));
        assert!(within_growth(4000, 4200, 50));
        assert!(!within_growth(4000, 4201, 50));

        let corpus: Corpus = serde_json::from_str(CORPUS_JSON).unwrap();
        let tokenizer = o200k_base().unwrap();
        let gates = calculate_gates(&corpus, &tokenizer).unwrap();
        assert_eq!(gates.eligible_success_count, 3);
        assert_eq!(
            gates.singular_eligible.final_tokens,
            tokenizer.count_ordinary(&complete_eligible_context(&corpus, "singular").unwrap())
        );
        assert_eq!(
            gates.bounded_eligible.final_tokens,
            tokenizer.count_ordinary(&complete_eligible_context(&corpus, "bounded").unwrap())
        );
        assert!(gates.eligible_success_round_trips);
        assert!(!gates.recovery_no_cost);
        assert!(!gates.passed());
    }
}
