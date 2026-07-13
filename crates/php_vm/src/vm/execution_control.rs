use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExceptionHandler {
    pub(super) catch: Option<BlockId>,
    pub(super) catch_types: Vec<String>,
    pub(super) finally: Option<BlockId>,
    pub(super) after: BlockId,
    pub(super) exception_local: Option<LocalId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PendingControl {
    Return(Option<Value>),
    Throw(Value),
}

/// Outcome of routing a runtime error through the active exception handlers.
pub(super) enum RaiseOutcome {
    /// A handler in the current frame caught the throwable; resume at this block.
    Caught(BlockId),
    /// Nothing caught it in this frame; return this result to the caller.
    Done(Box<VmResult>),
}

const EXECUTION_DEADLINE_CHECK_INTERVAL: usize = 64;
const EXECUTION_TIMEOUT_DIAGNOSTIC_ID: &str = "E_PHP_VM_EXECUTION_TIMEOUT";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExecutionLimitExceeded {
    Timeout,
    StepLimit,
}

pub(super) fn next_block_id(function: &IrFunction, current: BlockId) -> Result<BlockId, String> {
    let next = current.raw() + 1;
    if next as usize >= function.blocks.len() {
        return Err(format!(
            "fallthrough block after block:{} is missing",
            current.raw()
        ));
    }
    Ok(BlockId::new(next))
}

pub(super) fn execution_limit_exceeded(
    state: &ExecutionState,
    steps: usize,
    max_steps: usize,
) -> Option<ExecutionLimitExceeded> {
    let deadline_expired = steps.is_multiple_of(EXECUTION_DEADLINE_CHECK_INTERVAL)
        && state.execution_deadline_expired();
    classify_execution_limit(steps, max_steps, deadline_expired)
}

fn classify_execution_limit(
    steps: usize,
    max_steps: usize,
    deadline_expired: bool,
) -> Option<ExecutionLimitExceeded> {
    if deadline_expired {
        return Some(ExecutionLimitExceeded::Timeout);
    }
    (steps > max_steps).then_some(ExecutionLimitExceeded::StepLimit)
}

impl ExecutionState {
    pub(super) fn execution_deadline_expired(&self) -> bool {
        match self.execution_deadline_at {
            Some(deadline) => Instant::now() >= deadline,
            None => false,
        }
    }

    pub(super) fn reset_execution_deadline_seconds(&mut self, seconds: u64) {
        if !self.execution_deadline_mutable {
            return;
        }
        self.execution_deadline_at = if seconds == 0 {
            None
        } else {
            Instant::now().checked_add(Duration::from_secs(seconds))
        };
    }
}

impl Vm {
    pub(super) fn execution_timeout(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
    ) -> VmResult {
        self.runtime_error(
            output,
            compiled,
            stack,
            format!("{EXECUTION_TIMEOUT_DIAGNOSTIC_ID}: maximum execution time exceeded"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionLimitExceeded, classify_execution_limit};

    #[test]
    fn timeout_has_priority_when_both_limits_are_exceeded() {
        assert_eq!(
            classify_execution_limit(65, 64, true),
            Some(ExecutionLimitExceeded::Timeout)
        );
    }

    #[test]
    fn step_limit_and_within_limit_results_are_deterministic() {
        assert_eq!(
            classify_execution_limit(65, 64, false),
            Some(ExecutionLimitExceeded::StepLimit)
        );
        assert_eq!(classify_execution_limit(64, 64, false), None);
    }
}
