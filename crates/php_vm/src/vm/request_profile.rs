use super::Vm;
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub(super) struct RequestProfileFrame {
    child_nanos: u64,
}

#[derive(Clone, Debug)]
pub(super) struct RequestProfileBoundary {
    start: Instant,
    rich_instructions: u64,
    dense_instructions: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RequestProfileOperationCategory {
    Array,
    Object,
    Output,
}

#[derive(Debug)]
pub(super) struct RequestProfileOperation<'vm> {
    vm: &'vm Vm,
    category: RequestProfileOperationCategory,
    family: &'static str,
    start: Option<Instant>,
}

#[derive(Clone, Debug)]
struct RequestProfileSample {
    inclusive_nanos: u64,
    exclusive_nanos: u64,
    rich_instructions: u64,
    dense_instructions: u64,
}

impl Vm {
    pub(super) fn request_profile_boundary_start(&self) -> Option<RequestProfileBoundary> {
        if !self.options.collect_counters {
            return None;
        }
        let (rich_instructions, dense_instructions) = self.request_profile_instruction_snapshot();
        self.request_profile_stack
            .borrow_mut()
            .push(RequestProfileFrame::default());
        Some(RequestProfileBoundary {
            start: Instant::now(),
            rich_instructions,
            dense_instructions,
        })
    }

    pub(super) fn record_counter_function_profile(
        &self,
        name: &str,
        is_method: bool,
        boundary: Option<RequestProfileBoundary>,
    ) {
        let Some(sample) = self.request_profile_boundary_finish(boundary) else {
            return;
        };
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_function_profile(
                name,
                is_method,
                sample.inclusive_nanos,
                sample.exclusive_nanos,
                sample.rich_instructions,
                sample.dense_instructions,
            );
        }
    }

    pub(super) fn record_counter_builtin_profile(
        &self,
        name: &str,
        boundary: Option<RequestProfileBoundary>,
    ) {
        let Some(sample) = self.request_profile_boundary_finish(boundary) else {
            return;
        };
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_builtin_profile(
                name,
                sample.inclusive_nanos,
                sample.exclusive_nanos,
                sample.rich_instructions,
                sample.dense_instructions,
            );
        }
    }

    pub(super) fn record_counter_include_profile(
        &self,
        path: &str,
        boundary: Option<RequestProfileBoundary>,
    ) {
        let Some(sample) = self.request_profile_boundary_finish(boundary) else {
            return;
        };
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_include_profile(
                path,
                sample.inclusive_nanos,
                sample.exclusive_nanos,
                sample.rich_instructions,
                sample.dense_instructions,
            );
        }
    }

    pub(super) fn profile_builtin_call<T>(&self, name: &str, call: impl FnOnce() -> T) -> T {
        let boundary = self.request_profile_boundary_start();
        let result = call();
        self.record_counter_builtin_profile(name, boundary);
        result
    }

    pub(super) fn request_profile_operation_start(
        &self,
        category: RequestProfileOperationCategory,
        family: &'static str,
    ) -> RequestProfileOperation<'_> {
        RequestProfileOperation {
            vm: self,
            category,
            family,
            start: self.options.collect_counters.then(Instant::now),
        }
    }

    fn record_counter_operation_profile(
        &self,
        category: RequestProfileOperationCategory,
        family: &str,
        inclusive_nanos: u64,
    ) {
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match category {
                RequestProfileOperationCategory::Array => {
                    counters.record_array_operation_profile(family, inclusive_nanos);
                }
                RequestProfileOperationCategory::Object => {
                    counters.record_object_operation_profile(family, inclusive_nanos);
                }
                RequestProfileOperationCategory::Output => {
                    counters.record_output_operation_profile(family, inclusive_nanos);
                }
            }
        }
    }

    fn request_profile_instruction_snapshot(&self) -> (u64, u64) {
        let counters = self.counters.borrow();
        counters
            .as_ref()
            .map(|counters| {
                (
                    counters.instructions_executed,
                    counters.bytecode_instructions_executed,
                )
            })
            .unwrap_or((0, 0))
    }

    fn request_profile_boundary_finish(
        &self,
        boundary: Option<RequestProfileBoundary>,
    ) -> Option<RequestProfileSample> {
        let boundary = boundary?;
        let inclusive_nanos = elapsed_nanos(boundary.start);
        let child_nanos = self
            .request_profile_stack
            .borrow_mut()
            .pop()
            .map(|frame| frame.child_nanos)
            .unwrap_or_default();
        if let Some(parent) = self.request_profile_stack.borrow_mut().last_mut() {
            parent.child_nanos = parent.child_nanos.saturating_add(inclusive_nanos);
        }
        let (rich_instructions, dense_instructions) = self.request_profile_instruction_snapshot();
        Some(RequestProfileSample {
            inclusive_nanos,
            exclusive_nanos: inclusive_nanos.saturating_sub(child_nanos),
            rich_instructions: rich_instructions.saturating_sub(boundary.rich_instructions),
            dense_instructions: dense_instructions.saturating_sub(boundary.dense_instructions),
        })
    }
}

fn elapsed_nanos(start: Instant) -> u64 {
    let nanos = start.elapsed().as_nanos();
    nanos.min(u128::from(u64::MAX)) as u64
}

impl Drop for RequestProfileOperation<'_> {
    fn drop(&mut self) {
        let Some(start) = self.start.take() else {
            return;
        };
        self.vm
            .record_counter_operation_profile(self.category, self.family, elapsed_nanos(start));
    }
}
