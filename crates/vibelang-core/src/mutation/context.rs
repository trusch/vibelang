use super::{
    AttemptId, MutationContextWire, MutationReceipt, ReceiptEvent, RevisionId, RuntimeEpoch,
};
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct MutationReplySink(Arc<dyn Fn(MutationReceipt) + Send + Sync + 'static>);

impl MutationReplySink {
    #[must_use]
    pub fn new(send: impl Fn(MutationReceipt) + Send + Sync + 'static) -> Self {
        Self(Arc::new(send))
    }

    #[must_use]
    pub fn noop() -> Self {
        Self::new(|_| {})
    }

    pub fn publish(&self, receipt: MutationReceipt) {
        (self.0)(receipt);
    }
}

impl Default for MutationReplySink {
    fn default() -> Self {
        Self::noop()
    }
}

impl fmt::Debug for MutationReplySink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MutationReplySink(<redacted>)")
    }
}

#[derive(Clone)]
pub struct MutationEventSink(Arc<dyn Fn(ReceiptEvent) + Send + Sync + 'static>);

impl MutationEventSink {
    #[must_use]
    pub fn new(send: impl Fn(ReceiptEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(send))
    }

    #[must_use]
    pub fn noop() -> Self {
        Self::new(|_| {})
    }

    pub fn publish(&self, event: ReceiptEvent) {
        (self.0)(event);
    }
}

impl Default for MutationEventSink {
    fn default() -> Self {
        Self::noop()
    }
}

impl fmt::Debug for MutationEventSink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MutationEventSink(<redacted>)")
    }
}

/// Correlation and reply/event sinks preserved across internal mutation work.
#[derive(Clone, Debug)]
pub struct MutationContext {
    wire: MutationContextWire,
    reply_sink: MutationReplySink,
    event_sink: MutationEventSink,
}

impl MutationContext {
    #[must_use]
    pub fn new(
        attempt_id: AttemptId,
        runtime_epoch: RuntimeEpoch,
        idempotency_keyed: bool,
        reply_sink: MutationReplySink,
        event_sink: MutationEventSink,
    ) -> Self {
        Self {
            wire: MutationContextWire {
                attempt_id,
                runtime_epoch,
                revision: None,
                component_path: None,
                idempotency_keyed,
            },
            reply_sink,
            event_sink,
        }
    }

    pub fn from_wire(
        wire: MutationContextWire,
        reply_sink: MutationReplySink,
        event_sink: MutationEventSink,
    ) -> Self {
        Self {
            wire,
            reply_sink,
            event_sink,
        }
    }

    #[must_use]
    pub fn wire(&self) -> MutationContextWire {
        self.wire.clone()
    }

    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.wire.attempt_id
    }

    #[must_use]
    pub const fn runtime_epoch(&self) -> RuntimeEpoch {
        self.wire.runtime_epoch
    }

    #[must_use]
    pub const fn revision(&self) -> Option<RevisionId> {
        self.wire.revision
    }

    pub fn with_revision(&self, revision: RevisionId) -> Result<Self, String> {
        if self
            .wire
            .revision
            .is_some_and(|current| current != revision)
        {
            return Err("MutationContext revision is immutable once assigned".into());
        }
        let mut child = self.clone();
        child.wire.revision = Some(revision);
        Ok(child)
    }

    #[must_use]
    pub fn for_component(&self, component_path: impl Into<String>) -> Self {
        let mut child = self.clone();
        child.wire.component_path = Some(component_path.into());
        child
    }

    pub fn reply(&self, receipt: MutationReceipt) {
        self.reply_sink.publish(receipt);
    }

    pub fn event(&self, event: ReceiptEvent) {
        self.event_sink.publish(event);
    }
}
