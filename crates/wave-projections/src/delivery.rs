use anyhow::Context;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use wave_config::ProjectConfig;
use wave_domain::AcceptancePackageState;
use wave_domain::DeliveryCatalog;
use wave_domain::DeliverySeverity;
use wave_domain::InitiativeState;
use wave_domain::ReleaseState;
use wave_domain::SoftState;
use wave_spec::WaveDocument;

use crate::PlanningStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeliverySummaryReadModel {
    pub initiative_count: usize,
    pub release_count: usize,
    pub acceptance_package_count: usize,
    pub blocking_risk_count: usize,
    pub blocking_debt_count: usize,
    pub ready_release_count: usize,
    pub blocked_release_count: usize,
    pub accepted_package_count: usize,
    pub rejected_package_count: usize,
    pub advisory_count: usize,
    pub degraded_count: usize,
    pub stale_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct InitiativeReadModel {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub state: Option<InitiativeState>,
    pub soft_state: SoftState,
    pub owners: Vec<String>,
    pub wave_ids: Vec<u32>,
    pub release_ids: Vec<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ReleaseReadModel {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub initiative_id: Option<String>,
    pub state: Option<ReleaseState>,
    pub soft_state: SoftState,
    pub owners: Vec<String>,
    pub wave_ids: Vec<u32>,
    pub acceptance_package_ids: Vec<String>,
    pub milestone_id: Option<String>,
    pub release_train_id: Option<String>,
    pub blocking_risk_ids: Vec<String>,
    pub blocking_debt_ids: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct AcceptancePackageReadModel {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub release_id: Option<String>,
    pub state: Option<AcceptancePackageState>,
    pub soft_state: SoftState,
    pub wave_ids: Vec<u32>,
    pub proof_artifacts: Vec<String>,
    pub design_evidence: Vec<String>,
    pub documentation_evidence: Vec<String>,
    pub signoffs: Vec<String>,
    pub blocking_risk_ids: Vec<String>,
    pub blocking_debt_ids: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub ship_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeliveryRiskReadModel {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub severity: Option<DeliverySeverity>,
    pub soft_state: SoftState,
    pub release_id: Option<String>,
    pub acceptance_package_id: Option<String>,
    pub wave_ids: Vec<u32>,
    pub owners: Vec<String>,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeliveryDebtReadModel {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub severity: Option<DeliverySeverity>,
    pub soft_state: SoftState,
    pub release_id: Option<String>,
    pub acceptance_package_id: Option<String>,
    pub wave_ids: Vec<u32>,
    pub owners: Vec<String>,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeliverySignalReadModel {
    pub exit_code: i32,
    pub queue_state: String,
    pub delivery_soft_state: SoftState,
    pub next_claimable_wave_id: Option<u32>,
    pub ready_wave_ids: Vec<u32>,
    pub blocked_wave_ids: Vec<u32>,
    pub active_wave_ids: Vec<u32>,
    pub ready_release_ids: Vec<String>,
    pub blocked_release_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeliveryReadModel {
    pub summary: DeliverySummaryReadModel,
    pub signal: DeliverySignalReadModel,
    pub initiatives: Vec<InitiativeReadModel>,
    pub releases: Vec<ReleaseReadModel>,
    pub acceptance_packages: Vec<AcceptancePackageReadModel>,
    pub risks: Vec<DeliveryRiskReadModel>,
    pub debts: Vec<DeliveryDebtReadModel>,
    pub attention_lines: Vec<String>,
    #[serde(skip)]
    pub wave_soft_states: HashMap<u32, SoftState>,
}

pub fn load_delivery_catalog(root: &Path, config: &ProjectConfig) -> Result<DeliveryCatalog> {
    let path = config.resolved_paths(root).delivery_catalog_path;
    if !path.exists() {
        return Ok(DeliveryCatalog::default());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read delivery catalog {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(DeliveryCatalog::default());
    }
    let catalog = serde_json::from_str::<DeliveryCatalog>(&raw)
        .with_context(|| format!("failed to parse delivery catalog {}", path.display()))?;
    Ok(catalog)
}

pub fn build_delivery_read_model(
    status: &PlanningStatus,
    waves: &[WaveDocument],
    catalog: &DeliveryCatalog,
) -> DeliveryReadModel {
    let risks = catalog
        .risks
        .iter()
        .map(|risk| DeliveryRiskReadModel {
            id: risk.id.to_string(),
            title: risk.title.clone(),
            summary: risk.summary.clone(),
            severity: risk.severity,
            soft_state: risk.soft_state,
            release_id: risk.release_id.as_ref().map(ToString::to_string),
            acceptance_package_id: risk
                .acceptance_package_id
                .as_ref()
                .map(ToString::to_string),
            wave_ids: risk.wave_ids.clone(),
            owners: risk.owners.clone(),
            blocking: risk.severity.map(DeliverySeverity::is_blocking).unwrap_or(false),
        })
        .collect::<Vec<_>>();
    let debts = catalog
        .debts
        .iter()
        .map(|debt| DeliveryDebtReadModel {
            id: debt.id.to_string(),
            title: debt.title.clone(),
            summary: debt.summary.clone(),
            severity: debt.severity,
            soft_state: debt.soft_state,
            release_id: debt.release_id.as_ref().map(ToString::to_string),
            acceptance_package_id: debt
                .acceptance_package_id
                .as_ref()
                .map(ToString::to_string),
            wave_ids: debt.wave_ids.clone(),
            owners: debt.owners.clone(),
            blocking: debt.severity.map(DeliverySeverity::is_blocking).unwrap_or(false),
        })
        .collect::<Vec<_>>();

    let release_soft_overlays = catalog
        .releases
        .iter()
        .map(|release| {
            let mut soft_state = release.soft_state;
            for risk in &risks {
                if risk.release_id.as_deref() == Some(release.id.as_str()) {
                    soft_state = soft_state.merge(risk.soft_state);
                }
            }
            for debt in &debts {
                if debt.release_id.as_deref() == Some(release.id.as_str()) {
                    soft_state = soft_state.merge(debt.soft_state);
                }
            }
            (release.id.to_string(), soft_state)
        })
        .collect::<HashMap<_, _>>();

    let releases = catalog
        .releases
        .iter()
        .map(|release| {
            let mut blocking_risk_ids = release
                .blocking_risk_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let mut blocking_debt_ids = release
                .blocking_debt_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            for risk in &risks {
                if risk.blocking
                    && risk.release_id.as_deref() == Some(release.id.as_str())
                    && !blocking_risk_ids.iter().any(|id| id == &risk.id)
                {
                    blocking_risk_ids.push(risk.id.clone());
                }
            }
            for debt in &debts {
                if debt.blocking
                    && debt.release_id.as_deref() == Some(release.id.as_str())
                    && !blocking_debt_ids.iter().any(|id| id == &debt.id)
                {
                    blocking_debt_ids.push(debt.id.clone());
                }
            }

            let mut blocked_reasons = Vec::new();
            if !matches!(release.state, Some(ReleaseState::Ready | ReleaseState::Shipped)) {
                blocked_reasons.push(format!(
                    "state={}",
                    release
                        .state
                        .map(release_state_label)
                        .unwrap_or("unspecified")
                ));
            }
            for risk_id in &blocking_risk_ids {
                blocked_reasons.push(format!("blocking risk {}", risk_id));
            }
            for debt_id in &blocking_debt_ids {
                blocked_reasons.push(format!("blocking debt {}", debt_id));
            }
            let ready = matches!(release.state, Some(ReleaseState::Ready | ReleaseState::Shipped))
                && blocking_risk_ids.is_empty()
                && blocking_debt_ids.is_empty();

            ReleaseReadModel {
                id: release.id.to_string(),
                title: release.title.clone(),
                summary: release.summary.clone(),
                initiative_id: release.initiative_id.as_ref().map(ToString::to_string),
                state: release.state,
                soft_state: *release_soft_overlays
                    .get(release.id.as_str())
                    .unwrap_or(&release.soft_state),
                owners: release.owners.clone(),
                wave_ids: release.wave_ids.clone(),
                acceptance_package_ids: release
                    .acceptance_package_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                milestone_id: release.milestone_id.clone(),
                release_train_id: release.release_train_id.clone(),
                blocking_risk_ids,
                blocking_debt_ids,
                blocked_reasons,
                ready,
            }
        })
        .collect::<Vec<_>>();

    let acceptance_soft_overlays = catalog
        .acceptance_packages
        .iter()
        .map(|acceptance| {
            let mut soft_state = acceptance.soft_state;
            for risk in &risks {
                if risk.acceptance_package_id.as_deref() == Some(acceptance.id.as_str()) {
                    soft_state = soft_state.merge(risk.soft_state);
                }
            }
            for debt in &debts {
                if debt.acceptance_package_id.as_deref() == Some(acceptance.id.as_str()) {
                    soft_state = soft_state.merge(debt.soft_state);
                }
            }
            (acceptance.id.to_string(), soft_state)
        })
        .collect::<HashMap<_, _>>();

    let acceptance_packages = catalog
        .acceptance_packages
        .iter()
        .map(|acceptance| {
            let mut blocking_risk_ids = acceptance
                .blocking_risk_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let mut blocking_debt_ids = acceptance
                .blocking_debt_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            for risk in &risks {
                if risk.blocking
                    && risk.acceptance_package_id.as_deref() == Some(acceptance.id.as_str())
                    && !blocking_risk_ids.iter().any(|id| id == &risk.id)
                {
                    blocking_risk_ids.push(risk.id.clone());
                }
            }
            for debt in &debts {
                if debt.blocking
                    && debt.acceptance_package_id.as_deref() == Some(acceptance.id.as_str())
                    && !blocking_debt_ids.iter().any(|id| id == &debt.id)
                {
                    blocking_debt_ids.push(debt.id.clone());
                }
            }

            let mut blocked_reasons = Vec::new();
            if !matches!(acceptance.state, Some(AcceptancePackageState::Accepted)) {
                blocked_reasons.push(format!(
                    "state={}",
                    acceptance
                        .state
                        .map(acceptance_state_label)
                        .unwrap_or("unspecified")
                ));
            }
            for risk_id in &blocking_risk_ids {
                blocked_reasons.push(format!("blocking risk {}", risk_id));
            }
            for debt_id in &blocking_debt_ids {
                blocked_reasons.push(format!("blocking debt {}", debt_id));
            }
            let ship_ready = matches!(acceptance.state, Some(AcceptancePackageState::Accepted))
                && blocking_risk_ids.is_empty()
                && blocking_debt_ids.is_empty();

            AcceptancePackageReadModel {
                id: acceptance.id.to_string(),
                title: acceptance.title.clone(),
                summary: acceptance.summary.clone(),
                release_id: acceptance.release_id.as_ref().map(ToString::to_string),
                state: acceptance.state,
                soft_state: *acceptance_soft_overlays
                    .get(acceptance.id.as_str())
                    .unwrap_or(&acceptance.soft_state),
                wave_ids: acceptance.wave_ids.clone(),
                proof_artifacts: acceptance.proof_artifacts.clone(),
                design_evidence: acceptance.design_evidence.clone(),
                documentation_evidence: acceptance.documentation_evidence.clone(),
                signoffs: acceptance.signoffs.clone(),
                blocking_risk_ids,
                blocking_debt_ids,
                blocked_reasons,
                ship_ready,
            }
        })
        .collect::<Vec<_>>();

    let initiative_soft_overlays = catalog
        .initiatives
        .iter()
        .map(|initiative| {
            let mut soft_state = initiative.soft_state;
            for release in &releases {
                if initiative
                    .release_ids
                    .iter()
                    .any(|release_id| release_id.as_str() == release.id)
                {
                    soft_state = soft_state.merge(release.soft_state);
                }
            }
            (initiative.id.to_string(), soft_state)
        })
        .collect::<HashMap<_, _>>();

    let initiatives = catalog
        .initiatives
        .iter()
        .map(|initiative| InitiativeReadModel {
            id: initiative.id.to_string(),
            title: initiative.title.clone(),
            summary: initiative.summary.clone(),
            state: initiative.state,
            soft_state: *initiative_soft_overlays
                .get(initiative.id.as_str())
                .unwrap_or(&initiative.soft_state),
            owners: initiative.owners.clone(),
            wave_ids: initiative.wave_ids.clone(),
            release_ids: initiative.release_ids.iter().map(ToString::to_string).collect(),
            outcome: initiative.outcome.clone(),
        })
        .collect::<Vec<_>>();

    let mut wave_soft_states = status
        .waves
        .iter()
        .map(|wave| (wave.id, SoftState::Clear))
        .collect::<HashMap<_, _>>();
    for wave in waves {
        let wave_id = wave.metadata.id;
        let mut soft_state = SoftState::Clear;
        if let Some(delivery) = wave.metadata.delivery.as_ref() {
            if let Some(initiative_id) = delivery.initiative_id.as_deref() {
                if let Some(initiative) = initiatives.iter().find(|initiative| initiative.id == initiative_id)
                {
                    soft_state = soft_state.merge(initiative.soft_state);
                }
            }
            if let Some(release_id) = delivery.release_id.as_deref() {
                if let Some(release) = releases.iter().find(|release| release.id == release_id) {
                    soft_state = soft_state.merge(release.soft_state);
                }
            }
            if let Some(acceptance_id) = delivery.acceptance_package_id.as_deref() {
                if let Some(acceptance) = acceptance_packages
                    .iter()
                    .find(|acceptance| acceptance.id == acceptance_id)
                {
                    soft_state = soft_state.merge(acceptance.soft_state);
                }
            }
        }
        for risk in &risks {
            if risk.wave_ids.iter().any(|candidate| *candidate == wave_id) {
                soft_state = soft_state.merge(risk.soft_state);
            }
        }
        for debt in &debts {
            if debt.wave_ids.iter().any(|candidate| *candidate == wave_id) {
                soft_state = soft_state.merge(debt.soft_state);
            }
        }
        wave_soft_states.insert(wave_id, soft_state);
    }

    let mut attention_lines = Vec::new();
    for release in &releases {
        if !release.ready || release.soft_state != SoftState::Clear {
            attention_lines.push(format!(
                "delivery release {} {} state={} soft={} blockers={}",
                release.id,
                release.title,
                release
                    .state
                    .map(release_state_label)
                    .unwrap_or("unspecified"),
                release.soft_state.label(),
                format_string_list(&release.blocked_reasons)
            ));
        }
    }
    for acceptance in &acceptance_packages {
        if !acceptance.ship_ready || acceptance.soft_state != SoftState::Clear {
            attention_lines.push(format!(
                "delivery acceptance {} {} state={} soft={} blockers={}",
                acceptance.id,
                acceptance.title,
                acceptance
                    .state
                    .map(acceptance_state_label)
                    .unwrap_or("unspecified"),
                acceptance.soft_state.label(),
                format_string_list(&acceptance.blocked_reasons)
            ));
        }
    }

    let delivery_soft_state = initiatives
        .iter()
        .map(|initiative| initiative.soft_state)
        .chain(releases.iter().map(|release| release.soft_state))
        .chain(acceptance_packages.iter().map(|acceptance| acceptance.soft_state))
        .chain(risks.iter().map(|risk| risk.soft_state))
        .chain(debts.iter().map(|debt| debt.soft_state))
        .max()
        .unwrap_or(SoftState::Clear);

    let ready_release_ids = releases
        .iter()
        .filter(|release| release.ready)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    let blocked_release_ids = releases
        .iter()
        .filter(|release| !release.ready)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    let ready_wave_ids = status
        .waves
        .iter()
        .filter(|wave| wave.ready)
        .map(|wave| wave.id)
        .collect::<Vec<_>>();
    let blocked_wave_ids = status
        .waves
        .iter()
        .filter(|wave| !wave.ready && !wave.completed && !matches!(wave.readiness.state, crate::QueueReadinessState::Active | crate::QueueReadinessState::Claimed))
        .map(|wave| wave.id)
        .collect::<Vec<_>>();
    let active_wave_ids = status
        .waves
        .iter()
        .filter(|wave| matches!(wave.readiness.state, crate::QueueReadinessState::Active | crate::QueueReadinessState::Claimed))
        .map(|wave| wave.id)
        .collect::<Vec<_>>();
    let queue_state = if !active_wave_ids.is_empty() {
        "active".to_string()
    } else if !ready_wave_ids.is_empty() {
        "ready".to_string()
    } else if status.summary.completed_waves == status.summary.total_waves && status.summary.total_waves > 0 {
        "completed".to_string()
    } else {
        "blocked".to_string()
    };
    let exit_code = match delivery_soft_state {
        SoftState::Stale => 5,
        SoftState::Degraded => 4,
        SoftState::Advisory => 3,
        SoftState::Clear if !active_wave_ids.is_empty() => 2,
        SoftState::Clear if ready_wave_ids.is_empty() && !blocked_wave_ids.is_empty() => 1,
        SoftState::Clear => 0,
    };

    let summary = DeliverySummaryReadModel {
        initiative_count: initiatives.len(),
        release_count: releases.len(),
        acceptance_package_count: acceptance_packages.len(),
        blocking_risk_count: risks.iter().filter(|risk| risk.blocking).count(),
        blocking_debt_count: debts.iter().filter(|debt| debt.blocking).count(),
        ready_release_count: ready_release_ids.len(),
        blocked_release_count: blocked_release_ids.len(),
        accepted_package_count: acceptance_packages
            .iter()
            .filter(|acceptance| matches!(acceptance.state, Some(AcceptancePackageState::Accepted)))
            .count(),
        rejected_package_count: acceptance_packages
            .iter()
            .filter(|acceptance| matches!(acceptance.state, Some(AcceptancePackageState::Rejected)))
            .count(),
        advisory_count: count_soft_state(&initiatives, &releases, &acceptance_packages, &risks, &debts, SoftState::Advisory),
        degraded_count: count_soft_state(&initiatives, &releases, &acceptance_packages, &risks, &debts, SoftState::Degraded),
        stale_count: count_soft_state(&initiatives, &releases, &acceptance_packages, &risks, &debts, SoftState::Stale),
    };

    let signal = DeliverySignalReadModel {
        exit_code,
        queue_state,
        delivery_soft_state,
        next_claimable_wave_id: status.queue.next_ready_wave_id,
        ready_wave_ids,
        blocked_wave_ids,
        active_wave_ids,
        ready_release_ids,
        blocked_release_ids,
        message: format!(
            "queue={} delivery_soft={} releases_ready={}/{}",
            signal_queue_label(status),
            delivery_soft_state.label(),
            summary.ready_release_count,
            summary.release_count
        ),
    };

    DeliveryReadModel {
        summary,
        signal,
        initiatives,
        releases,
        acceptance_packages,
        risks,
        debts,
        attention_lines,
        wave_soft_states,
    }
}

fn count_soft_state(
    initiatives: &[InitiativeReadModel],
    releases: &[ReleaseReadModel],
    acceptance_packages: &[AcceptancePackageReadModel],
    risks: &[DeliveryRiskReadModel],
    debts: &[DeliveryDebtReadModel],
    target: SoftState,
) -> usize {
    initiatives
        .iter()
        .filter(|item| item.soft_state == target)
        .count()
        + releases.iter().filter(|item| item.soft_state == target).count()
        + acceptance_packages
            .iter()
            .filter(|item| item.soft_state == target)
            .count()
        + risks.iter().filter(|item| item.soft_state == target).count()
        + debts.iter().filter(|item| item.soft_state == target).count()
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn signal_queue_label(status: &PlanningStatus) -> &'static str {
    if status.summary.active_waves > 0 {
        "active"
    } else if status.summary.ready_waves > 0 {
        "ready"
    } else if status.summary.completed_waves == status.summary.total_waves && status.summary.total_waves > 0 {
        "completed"
    } else {
        "blocked"
    }
}

fn release_state_label(state: ReleaseState) -> &'static str {
    match state {
        ReleaseState::Planned => "planned",
        ReleaseState::Assembling => "assembling",
        ReleaseState::Candidate => "candidate",
        ReleaseState::Ready => "ready",
        ReleaseState::Shipped => "shipped",
        ReleaseState::Rejected => "rejected",
    }
}

fn acceptance_state_label(state: AcceptancePackageState) -> &'static str {
    match state {
        AcceptancePackageState::Draft => "draft",
        AcceptancePackageState::CollectingEvidence => "collecting-evidence",
        AcceptancePackageState::ReviewReady => "review-ready",
        AcceptancePackageState::Accepted => "accepted",
        AcceptancePackageState::Rejected => "rejected",
    }
}
