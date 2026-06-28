use std::collections::HashMap;

use chrono::{Duration, Utc};
use fractal_society::pkgs::research_project_validation::validate;
use fractal_society::protocol::{
    CostModel, DatasetBoundaries, DomainAdapterRef, EnvironmentManifest, Hash, MetricDefinition,
    MetricType, Protocol, ResearchProject, SafetyPolicy, WindowSpec,
};

fn window(start: i64, end: i64) -> WindowSpec {
    let epoch = chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap();
    WindowSpec {
        start: epoch + Duration::seconds(start),
        end: epoch + Duration::seconds(end),
        seed: 42,
    }
}

fn adapter() -> DomainAdapterRef {
    DomainAdapterRef {
        id: "trading".to_string(),
        version: "0.1.0".to_string(),
    }
}

fn valid_project() -> ResearchProject {
    let created_at = chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap();
    ResearchProject {
        id: "project-1".to_string(),
        version: "1.0.0".to_string(),
        question: "Can a deterministic strategy beat baseline?".to_string(),
        claim: "The candidate improves net return under S0 simulation.".to_string(),
        domain_adapter: adapter(),
        protocol: Protocol {
            id: "protocol-1".to_string(),
            version: "1.0.0".to_string(),
            agent_versions: HashMap::new(),
            allowed_tools: vec!["trade".to_string()],
            dataset_boundaries: DatasetBoundaries {
                development: window(0, 10),
                validation: window(10, 20),
                evaluation: window(20, 30),
            },
            primary_metrics: vec![MetricDefinition {
                name: "net_return".to_string(),
                higher_is_better: true,
                metric_type: MetricType::Percentage,
            }],
            cost_model: CostModel {
                fee_schedule: "5bps".to_string(),
                latency_ms: 0,
                slippage_model: "none".to_string(),
            },
            safety_policy: SafetyPolicy {
                max_drawdown: 0.1,
                max_leverage: 2.0,
                policy_violations_eq_zero: true,
            },
            required_verifiers: vec![],
        },
        datasets: HashMap::new(),
        environment: EnvironmentManifest {
            id: "env-1".to_string(),
            domain_adapter: adapter(),
            config: serde_json::json!({"fixture": "synthetic"}),
            version_hash: Hash::new(b"environment"),
        },
        created_at,
        updated_at: created_at,
    }
}

#[test]
fn valid_project_passes() {
    assert_eq!(validate(&valid_project()), Ok(()));
}

#[test]
fn empty_id_question_and_claim_fail() {
    let mut project = valid_project();
    project.id.clear();
    project.question = " ".to_string();
    project.claim.clear();

    let errors = validate(&project).expect_err("missing required fields should fail");

    assert!(errors.iter().any(|error| error == "id must be non-empty"));
    assert!(errors
        .iter()
        .any(|error| error == "question must be non-empty"));
    assert!(errors
        .iter()
        .any(|error| error == "claim must be non-empty"));
}

#[test]
fn empty_domain_adapter_id_fails() {
    let mut project = valid_project();
    project.domain_adapter.id.clear();

    let errors = validate(&project).expect_err("empty adapter id should fail");

    assert!(errors
        .iter()
        .any(|error| error == "domain_adapter.id must be non-empty"));
}

#[test]
fn empty_domain_adapter_version_fails() {
    let mut project = valid_project();
    project.domain_adapter.version.clear();

    let errors = validate(&project).expect_err("empty adapter version should fail");

    assert!(errors
        .iter()
        .any(|error| error == "domain_adapter.version must be non-empty"));
}
