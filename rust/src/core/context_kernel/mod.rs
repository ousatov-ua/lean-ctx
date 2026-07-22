//! Context Control Kernel — unified orchestration over all context stores.

pub mod a2a_fixes;
pub mod accounting_fix;
pub mod activation;
pub mod attribution;
pub mod bench;
pub mod bounded;
pub mod bridge;
pub mod capsule_wire;
pub mod client_e2e;
pub mod client_profile;
pub mod conformance;
pub mod context_broker;
pub mod context_dedup;
pub mod coverage_class;
pub mod degradation;
pub mod enforce;
pub mod etpao;
pub mod etpao_live;
pub mod feedback;
pub mod hotpath_wiring;
pub mod invalidation;
pub mod knowledge_health;
pub mod learning;
pub mod multi_agent_e2e;
pub mod orchestrator;
pub mod outcome_signal;
pub mod policy;
pub mod providers;
pub mod quality_e2e;
pub mod recovery;
pub mod result_fusion;
pub mod shadow;
pub mod types;

