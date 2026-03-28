use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use wave_config::ProjectConfig;
use wave_control_plane::build_planning_status;
use wave_spec::load_wave_documents;
use wave_spec::parse_wave_document;
use wave_trace::now_epoch_ms;

use super::LaunchOptions;
use super::LaunchReport;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdhocRunStatus {
    Planned,
    Launched,
    Failed,
    Promoted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdhocRequest {
    pub title: String,
    pub request: String,
    pub owner: Option<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdhocSpec {
    pub run_id: String,
    pub slug: String,
    pub title: String,
    pub owner: Option<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdhocResult {
    pub run_id: String,
    pub status: AdhocRunStatus,
    pub detail: Option<String>,
    pub bundle_dir: Option<String>,
    pub state_path: Option<String>,
    pub trace_path: Option<String>,
    pub promoted_wave_id: Option<u32>,
    pub promoted_path: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdhocRunRecord {
    pub run_id: String,
    pub request: AdhocRequest,
    pub spec: AdhocSpec,
    pub wave_path: String,
    pub runtime_dir: String,
    pub result: AdhocResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdhocPlanReport {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub request_path: PathBuf,
    pub spec_path: PathBuf,
    pub wave_path: PathBuf,
    pub result_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdhocRunReport {
    pub record: AdhocRunRecord,
    pub launch: LaunchReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdhocPromotionReport {
    pub record: AdhocRunRecord,
    pub promoted_wave_id: u32,
    pub promoted_path: PathBuf,
}

pub fn plan_adhoc(
    root: &Path,
    config: &ProjectConfig,
    title: &str,
    request: &str,
    owner: Option<&str>,
) -> Result<AdhocPlanReport> {
    let created_at_ms = now_epoch_ms()?;
    let slug = slugify(title);
    let run_id = format!("adhoc-{slug}-{created_at_ms}");
    let run_dir = adhoc_runs_dir(root, config).join(&run_id);
    let runtime_dir = adhoc_runtime_dir(root, config).join(&run_id);
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;
    fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create {}", runtime_dir.display()))?;

    let request_record = AdhocRequest {
        title: title.trim().to_string(),
        request: request.trim().to_string(),
        owner: owner.map(|owner| owner.trim().to_string()),
        created_at_ms,
    };
    let spec = AdhocSpec {
        run_id: run_id.clone(),
        slug,
        title: title.trim().to_string(),
        owner: owner.map(|owner| owner.trim().to_string()),
        created_at_ms,
    };
    let result = AdhocResult {
        run_id: run_id.clone(),
        status: AdhocRunStatus::Planned,
        detail: Some("ad hoc run planned".to_string()),
        bundle_dir: None,
        state_path: None,
        trace_path: None,
        promoted_wave_id: None,
        promoted_path: None,
        updated_at_ms: created_at_ms,
    };

    let request_path = run_dir.join("request.json");
    let spec_path = run_dir.join("spec.json");
    let wave_path = run_dir.join("wave-0.md");
    let result_path = run_dir.join("result.json");

    write_json(&request_path, &request_record)?;
    write_json(&spec_path, &spec)?;
    fs::write(
        &wave_path,
        render_adhoc_wave_document(&spec, &request_record),
    )
    .with_context(|| format!("failed to write {}", wave_path.display()))?;
    write_json(&result_path, &result)?;

    Ok(AdhocPlanReport {
        run_id,
        run_dir,
        runtime_dir,
        request_path,
        spec_path,
        wave_path,
        result_path,
    })
}

pub fn run_adhoc(root: &Path, config: &ProjectConfig, run_id: &str) -> Result<AdhocRunReport> {
    let mut record = read_adhoc_run_record(root, config, run_id)?;
    let wave_path = PathBuf::from(&record.wave_path);
    let contents = fs::read_to_string(&wave_path)
        .with_context(|| format!("failed to read {}", wave_path.display()))?;
    let wave = parse_wave_document(wave_path.clone(), &contents)?;

    let runtime_config = adhoc_runtime_config(config, run_id);
    runtime_config
        .resolved_paths(root)
        .authority
        .materialize_canonical_state_tree()?;
    fs::create_dir_all(
        runtime_config
            .resolved_paths(root)
            .authority
            .trace_runs_dir
            .clone(),
    )
    .with_context(|| {
        format!(
            "failed to create {}",
            runtime_config
                .resolved_paths(root)
                .authority
                .trace_runs_dir
                .display()
        )
    })?;

    record.result = AdhocResult {
        run_id: record.run_id.clone(),
        status: AdhocRunStatus::Launched,
        detail: Some("ad hoc launch entered isolated runtime execution".to_string()),
        bundle_dir: record.result.bundle_dir.clone(),
        state_path: record.result.state_path.clone(),
        trace_path: record.result.trace_path.clone(),
        promoted_wave_id: record.result.promoted_wave_id,
        promoted_path: record.result.promoted_path.clone(),
        updated_at_ms: now_epoch_ms()?,
    };
    write_adhoc_result(root, config, &record)?;

    let waves = vec![wave];
    let planning_status = build_planning_status(&runtime_config, &waves, &[], &HashMap::new());
    let launch = super::launch_wave(
        root,
        &runtime_config,
        &waves,
        &planning_status,
        LaunchOptions {
            wave_id: Some(0),
            dry_run: false,
        },
    );

    match launch {
        Ok(launch) => {
            record.result = AdhocResult {
                run_id: record.run_id.clone(),
                status: AdhocRunStatus::Launched,
                detail: Some("ad hoc run launched under isolated authority".to_string()),
                bundle_dir: Some(launch.bundle_dir.display().to_string()),
                state_path: Some(launch.state_path.display().to_string()),
                trace_path: Some(launch.trace_path.display().to_string()),
                promoted_wave_id: record.result.promoted_wave_id,
                promoted_path: record.result.promoted_path.clone(),
                updated_at_ms: now_epoch_ms()?,
            };
            write_adhoc_result(root, config, &record)?;
            Ok(AdhocRunReport { record, launch })
        }
        Err(error) => {
            record.result = AdhocResult {
                run_id: record.run_id.clone(),
                status: AdhocRunStatus::Failed,
                detail: Some(error.to_string()),
                bundle_dir: None,
                state_path: None,
                trace_path: None,
                promoted_wave_id: record.result.promoted_wave_id,
                promoted_path: record.result.promoted_path.clone(),
                updated_at_ms: now_epoch_ms()?,
            };
            write_adhoc_result(root, config, &record)?;
            Err(error)
        }
    }
}

pub fn list_adhoc_runs(root: &Path, config: &ProjectConfig) -> Result<Vec<AdhocRunRecord>> {
    let runs_dir = adhoc_runs_dir(root, config);
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut records = fs::read_dir(&runs_dir)
        .with_context(|| format!("failed to read {}", runs_dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let run_id = entry.file_name().to_string_lossy().to_string();
            read_adhoc_run_record(root, config, &run_id).ok()
        })
        .collect::<Vec<_>>();
    records.sort_by_key(|record| record.request.created_at_ms);
    records.reverse();
    Ok(records)
}

pub fn show_adhoc_run(root: &Path, config: &ProjectConfig, run_id: &str) -> Result<AdhocRunRecord> {
    read_adhoc_run_record(root, config, run_id)
}

pub fn promote_adhoc(
    root: &Path,
    config: &ProjectConfig,
    run_id: &str,
    target_wave_id: Option<u32>,
) -> Result<AdhocPromotionReport> {
    let mut record = read_adhoc_run_record(root, config, run_id)?;
    let wave_path = PathBuf::from(&record.wave_path);
    let raw = fs::read_to_string(&wave_path)
        .with_context(|| format!("failed to read {}", wave_path.display()))?;
    let next_wave_id = match target_wave_id {
        Some(wave_id) => wave_id,
        None => {
            load_wave_documents(config, root)?
                .into_iter()
                .map(|wave| wave.metadata.id)
                .max()
                .unwrap_or(0)
                + 1
        }
    };
    let promoted_path = root
        .join(&config.waves_dir)
        .join(format!("{next_wave_id:02}-{}.md", record.spec.slug));
    if promoted_path.exists() {
        bail!(
            "promoted wave path already exists: {}",
            promoted_path.display()
        );
    }

    let promoted = rewrite_adhoc_wave_for_promotion(&raw, &record.spec.title, next_wave_id);
    fs::write(&promoted_path, promoted)
        .with_context(|| format!("failed to write {}", promoted_path.display()))?;

    record.result = AdhocResult {
        run_id: record.run_id.clone(),
        status: AdhocRunStatus::Promoted,
        detail: Some("ad hoc run promoted into numbered waves/".to_string()),
        bundle_dir: record.result.bundle_dir.clone(),
        state_path: record.result.state_path.clone(),
        trace_path: record.result.trace_path.clone(),
        promoted_wave_id: Some(next_wave_id),
        promoted_path: Some(promoted_path.display().to_string()),
        updated_at_ms: now_epoch_ms()?,
    };
    write_adhoc_result(root, config, &record)?;

    Ok(AdhocPromotionReport {
        record,
        promoted_wave_id: next_wave_id,
        promoted_path,
    })
}

fn read_adhoc_run_record(
    root: &Path,
    config: &ProjectConfig,
    run_id: &str,
) -> Result<AdhocRunRecord> {
    let run_dir = adhoc_runs_dir(root, config).join(run_id);
    let request = read_json::<AdhocRequest>(&run_dir.join("request.json"))?;
    let spec = read_json::<AdhocSpec>(&run_dir.join("spec.json"))?;
    let result = read_json::<AdhocResult>(&run_dir.join("result.json"))?;
    let wave_path = run_dir.join("wave-0.md");
    Ok(AdhocRunRecord {
        run_id: run_id.to_string(),
        request,
        spec,
        wave_path: wave_path.display().to_string(),
        runtime_dir: adhoc_runtime_dir(root, config)
            .join(run_id)
            .display()
            .to_string(),
        result,
    })
}

fn write_adhoc_result(root: &Path, config: &ProjectConfig, record: &AdhocRunRecord) -> Result<()> {
    write_json(
        &adhoc_runs_dir(root, config)
            .join(&record.run_id)
            .join("result.json"),
        &record.result,
    )
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value)
}

fn adhoc_runtime_config(config: &ProjectConfig, run_id: &str) -> ProjectConfig {
    let mut cloned = config.clone();
    let runtime_root = config
        .authority
        .state_adhoc_dir
        .join("runtime")
        .join(run_id);
    cloned.authority.project_codex_home = runtime_root.join("codex");
    cloned.authority.state_dir = runtime_root.clone();
    cloned.authority.state_build_specs_dir = runtime_root.join("build/specs");
    cloned.authority.state_runs_dir = runtime_root.join("runs");
    cloned.authority.state_control_dir = runtime_root.join("control");
    cloned.authority.trace_dir = runtime_root.join("traces");
    cloned.authority.trace_runs_dir = runtime_root.join("traces/runs");
    cloned.authority.state_events_dir = runtime_root.join("events");
    cloned.authority.state_events_scheduler_dir = runtime_root.join("events/scheduler");
    cloned.authority.state_events_control_dir = runtime_root.join("events/control");
    cloned.authority.state_events_coordination_dir = runtime_root.join("events/coordination");
    cloned.authority.state_results_dir = runtime_root.join("results");
    cloned.authority.state_derived_dir = runtime_root.join("derived");
    cloned.authority.state_projections_dir = runtime_root.join("projections");
    cloned.authority.state_traces_dir = runtime_root.join("state-traces");
    cloned.authority.state_worktrees_dir = runtime_root.join("worktrees");
    cloned
}

fn render_adhoc_wave_document(spec: &AdhocSpec, request: &AdhocRequest) -> String {
    let owner = request
        .owner
        .as_deref()
        .filter(|owner| !owner.trim().is_empty())
        .unwrap_or("delivery");
    format!(
        r#"+++
id = 0
slug = "{slug}"
title = "{title}"
mode = "dark-factory"
owners = ["{owner}"]
validation = ["cargo test -q"]
rollback = ["Remove the promoted wave or discard the adhoc run if it should not persist."]
proof = ["Adhoc run artifacts under .wave/state/adhoc/runtime/{run_id}/"]
wave_class = "implementation"
intent = "investigation"
+++
# Wave 0 - {title}

**Commit message**: `Feat: promote adhoc wave {slug}`

## Component promotions
- adhoc-run: planned

## Deploy environments
- repo-local: isolated adhoc authority under `.wave/state/adhoc/runtime/{run_id}/`

## Context7 defaults
- bundle: rust-control-plane
- query: "Repo-local orchestration patterns, proof surfaces, and delivery-aware follow-through"

## Agent A1: Adhoc implementation owner

### Executor
- profile: implement-codex
- model: gpt-5.4

### Context7
- bundle: rust-control-plane
- query: "Implement the adhoc request with repo-local proof and promotion readiness"

### Skills
- wave-core
- role-implementation

### Components
- adhoc-run

### Capabilities
- investigation
- promotion-ready-doc

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- docs/plans/adhoc/{slug}.md

### File ownership
- docs/plans/adhoc/{slug}.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Execute the adhoc request and leave a promotion-ready artifact in docs/plans/adhoc/{slug}.md.

Required context before coding:
- Read README.md and docs/plans/product-factory-cutover.md.
- Treat the request below as the highest-priority scope.

Specific expectations:
- Request: {request_body}
- Produce concrete evidence and clear next steps.
- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.

File ownership (only touch these paths):
- docs/plans/adhoc/{slug}.md
```

## Agent A8: Integration closure

### Role prompts
- docs/agents/wave-integration-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: rust-control-plane
- query: "Integration closure checks for isolated adhoc outputs"

### Skills
- wave-core
- role-integration

### File ownership
- .wave/reviews/{run_id}-integration.md

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Judge whether the adhoc output is coherent and promotion-ready.

Required context before coding:
- Read docs/plans/adhoc/{slug}.md.

Specific expectations:
- Call out any integration gaps that would block promotion into waves/.
- Emit [wave-integration] on plain lines by themselves at the end of the output.

File ownership (only touch these paths):
- .wave/reviews/{run_id}-integration.md
```

## Agent A9: Documentation closure

### Role prompts
- docs/agents/wave-documentation-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: rust-control-plane
- query: "Documentation closure checks for promotion-ready adhoc outputs"

### Skills
- wave-core
- role-documentation

### File ownership
- .wave/reviews/{run_id}-docs.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Verify the adhoc output is documented clearly enough to promote into waves/.

Required context before coding:
- Read docs/plans/adhoc/{slug}.md.

Specific expectations:
- Identify any missing explanations, evidence, or operator notes.
- Emit [wave-doc-closure] on plain lines by themselves at the end of the output.

File ownership (only touch these paths):
- .wave/reviews/{run_id}-docs.md
```

## Agent A0: Cont-QA closure

### Role prompts
- docs/agents/wave-cont-qa-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: rust-control-plane
- query: "Gate whether the adhoc output is credible enough to promote"

### Skills
- wave-core
- role-cont-qa

### File ownership
- .wave/reviews/{run_id}-gate.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Decide whether this adhoc run produced enough evidence to promote into the numbered wave backlog.

Required context before coding:
- Read docs/plans/adhoc/{slug}.md plus the closure notes.

Specific expectations:
- Be strict about evidence, clarity, and promotion readiness.
- Emit [wave-gate] on plain lines by themselves at the end of the output.

File ownership (only touch these paths):
- .wave/reviews/{run_id}-gate.md
```
"#,
        slug = spec.slug,
        title = spec.title,
        owner = owner,
        run_id = spec.run_id,
        request_body = request.request.replace('\n', " "),
    )
}

fn rewrite_adhoc_wave_for_promotion(raw: &str, title: &str, wave_id: u32) -> String {
    let mut rewritten = Vec::new();
    for line in raw.lines() {
        if line.starts_with("id = ") {
            rewritten.push(format!("id = {wave_id}"));
        } else if line.starts_with("# Wave 0 -") {
            rewritten.push(format!("# Wave {wave_id} - {title}"));
        } else {
            rewritten.push(line.to_string());
        }
    }
    let mut result = rewritten.join("\n");
    result.push('\n');
    result
}

fn adhoc_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    root.join(&config.authority.state_adhoc_dir).join("runs")
}

fn adhoc_runtime_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    root.join(&config.authority.state_adhoc_dir).join("runtime")
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in title.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adhoc_request_fixture() -> AdhocRequest {
        AdhocRequest {
            title: "Smoke Test Adhoc".to_string(),
            request: "Validate the repo-local adhoc lane.".to_string(),
            owner: None,
            created_at_ms: 1,
        }
    }

    fn adhoc_spec_fixture() -> AdhocSpec {
        AdhocSpec {
            run_id: "adhoc-smoke-test-1".to_string(),
            slug: "smoke-test-adhoc".to_string(),
            title: "Smoke Test Adhoc".to_string(),
            owner: None,
            created_at_ms: 1,
        }
    }

    #[test]
    fn rendered_adhoc_wave_document_parses_as_wave_markdown() {
        let raw = render_adhoc_wave_document(&adhoc_spec_fixture(), &adhoc_request_fixture());

        assert!(raw.starts_with("+++\n"));
        assert!(raw.contains("\n+++\n# Wave 0 - Smoke Test Adhoc\n"));

        let parsed = parse_wave_document(
            Path::new(".wave/state/adhoc/runs/adhoc-smoke-test-1/wave-0.md").to_path_buf(),
            &raw,
        )
        .expect("adhoc wave document should parse");

        assert_eq!(parsed.metadata.id, 0);
        assert_eq!(parsed.metadata.slug, "smoke-test-adhoc");
        assert_eq!(
            parsed.metadata.wave_class,
            wave_spec::WaveClass::Implementation
        );
        assert_eq!(
            parsed.metadata.intent,
            Some(wave_spec::WaveIntent::Investigation)
        );
    }

    #[test]
    fn promote_rewrite_updates_wave_id_and_heading() {
        let raw = render_adhoc_wave_document(&adhoc_spec_fixture(), &adhoc_request_fixture());
        let promoted = rewrite_adhoc_wave_for_promotion(&raw, "Smoke Test Adhoc", 42);

        assert!(promoted.contains("\nid = 42\n"));
        assert!(promoted.contains("\n# Wave 42 - Smoke Test Adhoc\n"));
        assert!(promoted.starts_with("+++\n"));
    }

    #[test]
    fn rendered_adhoc_wave_document_passes_dark_factory_lint() {
        let raw = render_adhoc_wave_document(&adhoc_spec_fixture(), &adhoc_request_fixture());
        let parsed = parse_wave_document(
            Path::new(".wave/state/adhoc/runs/adhoc-smoke-test-1/wave-0.md").to_path_buf(),
            &raw,
        )
        .expect("adhoc wave document should parse");
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let findings = wave_dark_factory::lint_project(&workspace_root, &[parsed]);

        assert!(
            !wave_dark_factory::has_errors(&findings),
            "unexpected adhoc lint findings: {findings:#?}"
        );
    }
}
