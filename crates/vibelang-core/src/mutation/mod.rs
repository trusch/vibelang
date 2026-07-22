//! Canonical mutation identity, receipt, and ledger primitives.
//!
//! These types are deliberately not connected to the current v1 runtime yet.
//! M04 propagates [`MutationContext`] through the existing message graph and
//! projects current best-effort outcomes onto this canonical contract.

mod context;
mod digest;
mod external;
mod ledger;
mod wire;

pub use context::{MutationContext, MutationEventSink, MutationReplySink};
pub use digest::{DigestError, RedactionHook, RequestMaterial};
pub use external::{
    submit_external_effect, CandidateSubmission, ExternalEffectDomain, ExternalEffectError,
    ExternalEffectOperation, ExternalEffectSubmission, SequencedExternalPlan,
    SequencedSubmissionOutcome,
};
pub use ledger::{
    CancelResult, LedgerConfig, LedgerError, MutationLedger, Submission, SubmissionResult,
};
pub use wire::*;
