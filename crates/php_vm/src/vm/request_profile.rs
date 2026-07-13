use super::Vm;
use crate::counters::BoundaryWorkSnapshot;
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub(super) struct RequestProfileFrame {
    child_nanos: u64,
    child_rich_instructions: u64,
    child_dense_instructions: u64,
    child_work: BoundaryWorkSnapshot,
}

impl RequestProfileFrame {
    fn absorb_sample(&mut self, sample: &RequestProfileSample) {
        self.child_nanos = self.child_nanos.saturating_add(sample.inclusive_nanos);
        self.child_rich_instructions = self
            .child_rich_instructions
            .saturating_add(sample.inclusive_rich_instructions);
        self.child_dense_instructions = self
            .child_dense_instructions
            .saturating_add(sample.inclusive_dense_instructions);
        self.child_work.saturating_add_assign(sample.inclusive_work);
    }

    fn absorb_profiled_children(&mut self, discarded: Self) {
        self.child_nanos = self.child_nanos.saturating_add(discarded.child_nanos);
        self.child_rich_instructions = self
            .child_rich_instructions
            .saturating_add(discarded.child_rich_instructions);
        self.child_dense_instructions = self
            .child_dense_instructions
            .saturating_add(discarded.child_dense_instructions);
        self.child_work.saturating_add_assign(discarded.child_work);
    }
}

#[derive(Clone, Debug)]
pub(super) struct RequestProfileBoundary {
    start: Instant,
    rich_instructions: u64,
    dense_instructions: u64,
    work: BoundaryWorkSnapshot,
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

impl RequestProfileOperation<'_> {
    /// Prevents a speculative operation sample from being recorded when the
    /// fast path side-exits and the interpreter performs the real operation.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    pub(super) fn cancel(&mut self) {
        self.start = None;
    }
}

#[derive(Clone, Debug)]
struct RequestProfileSample {
    inclusive_nanos: u64,
    exclusive_nanos: u64,
    inclusive_rich_instructions: u64,
    exclusive_rich_instructions: u64,
    inclusive_dense_instructions: u64,
    exclusive_dense_instructions: u64,
    inclusive_work: BoundaryWorkSnapshot,
    exclusive_work: BoundaryWorkSnapshot,
}

impl Vm {
    pub(super) fn request_profile_boundary_start(&self) -> Option<RequestProfileBoundary> {
        if !self.options.collect_counters || !self.options.collect_profile_spans {
            return None;
        }
        let (rich_instructions, dense_instructions) = self.request_profile_instruction_snapshot();
        let work = self.request_profile_work_snapshot();
        self.request_profile_stack
            .borrow_mut()
            .push(RequestProfileFrame::default());
        Some(RequestProfileBoundary {
            start: Instant::now(),
            rich_instructions,
            dense_instructions,
            work,
        })
    }

    /// Abandons a speculative boundary when a fast path declines the call.
    pub(super) fn request_profile_boundary_discard(
        &self,
        boundary: Option<RequestProfileBoundary>,
    ) {
        if boundary.is_some() {
            let discarded = self
                .request_profile_stack
                .borrow_mut()
                .pop()
                .expect("active request-profile boundary");
            if let Some(parent) = self.request_profile_stack.borrow_mut().last_mut() {
                parent.absorb_profiled_children(discarded);
            }
        }
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
                sample.inclusive_rich_instructions,
                sample.exclusive_rich_instructions,
                sample.inclusive_dense_instructions,
                sample.exclusive_dense_instructions,
                sample.inclusive_work,
                sample.exclusive_work,
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
                sample.inclusive_rich_instructions,
                sample.exclusive_rich_instructions,
                sample.inclusive_dense_instructions,
                sample.exclusive_dense_instructions,
                sample.inclusive_work,
                sample.exclusive_work,
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
                sample.inclusive_rich_instructions,
                sample.exclusive_rich_instructions,
                sample.inclusive_dense_instructions,
                sample.exclusive_dense_instructions,
                sample.inclusive_work,
                sample.exclusive_work,
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
            start: (self.options.collect_counters && self.options.collect_profile_spans)
                .then(Instant::now),
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

    fn request_profile_work_snapshot(&self) -> BoundaryWorkSnapshot {
        let layout = php_runtime::experimental::layout_stats::snapshot_layout_stats();
        let numeric = php_runtime::experimental::numeric_string::snapshot_cache_stats();
        let counters = self.counters.borrow();
        let Some(counters) = counters.as_ref() else {
            return BoundaryWorkSnapshot::default();
        };
        BoundaryWorkSnapshot {
            value_clones: layout.value_clones,
            refcounted_value_clones: layout
                .value_clone_by_kind
                .iter()
                .enumerate()
                .filter(|(kind, _)| *kind != 0)
                .map(|(_, count)| *count)
                .sum(),
            string_allocations: layout.string_allocations,
            array_handle_clones: layout.array_handle_clones,
            cow_separations: layout.cow_separations,
            reference_cell_creations: layout.reference_cell_creations,
            frame_allocations: counters.frame_allocations,
            frame_reuses: counters.frame_reuses,
            register_files_allocated: counters.register_files_allocated,
            register_files_reused: counters.register_files_reused,
            internal_function_dispatches: counters.internal_function_dispatches,
            symbol_map_lookups: layout.symbol_map_lookups,
            symbol_linear_fallbacks: layout.symbol_linear_fallbacks,
            symbol_intern_hits: layout.symbol_intern_hits,
            symbol_intern_misses: layout.symbol_intern_misses,
            string_hash_cache_hits: layout.string_hash_cache_hits,
            string_hash_cache_misses: layout.string_hash_cache_misses,
            symbol_eq_fast_hits: layout.symbol_eq_fast_hits,
            symbol_eq_byte_fallbacks: layout.symbol_eq_byte_fallbacks,
            array_dim_fetches: counters.array_dim_fetches,
            numeric_string_classify_calls: numeric.classify_calls,
            object_allocations: layout.object_allocations,
            property_accesses: counters.property_accesses,
            includes: counters.includes,
            autoloads: counters.autoloads,
        }
    }

    fn request_profile_boundary_finish(
        &self,
        boundary: Option<RequestProfileBoundary>,
    ) -> Option<RequestProfileSample> {
        let boundary = boundary?;
        let inclusive_nanos = elapsed_nanos(boundary.start);
        let child = self
            .request_profile_stack
            .borrow_mut()
            .pop()
            .unwrap_or_default();
        let (rich_instructions, dense_instructions) = self.request_profile_instruction_snapshot();
        let sample = reconcile_boundary_sample(
            inclusive_nanos,
            boundary.rich_instructions,
            boundary.dense_instructions,
            boundary.work,
            rich_instructions,
            dense_instructions,
            self.request_profile_work_snapshot(),
            child,
        );
        if let Some(parent) = self.request_profile_stack.borrow_mut().last_mut() {
            parent.absorb_sample(&sample);
        }
        Some(sample)
    }
}

fn reconcile_boundary_sample(
    inclusive_nanos: u64,
    start_rich_instructions: u64,
    start_dense_instructions: u64,
    start_work: BoundaryWorkSnapshot,
    end_rich_instructions: u64,
    end_dense_instructions: u64,
    end_work: BoundaryWorkSnapshot,
    child: RequestProfileFrame,
) -> RequestProfileSample {
    let inclusive_rich_instructions = end_rich_instructions.saturating_sub(start_rich_instructions);
    let inclusive_dense_instructions =
        end_dense_instructions.saturating_sub(start_dense_instructions);
    let inclusive_work = end_work.saturating_sub(start_work);
    RequestProfileSample {
        inclusive_nanos,
        exclusive_nanos: inclusive_nanos.saturating_sub(child.child_nanos),
        inclusive_rich_instructions,
        exclusive_rich_instructions: inclusive_rich_instructions
            .saturating_sub(child.child_rich_instructions),
        inclusive_dense_instructions,
        exclusive_dense_instructions: inclusive_dense_instructions
            .saturating_sub(child.child_dense_instructions),
        inclusive_work,
        exclusive_work: inclusive_work.saturating_sub(child.child_work),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn work(value: u64) -> BoundaryWorkSnapshot {
        BoundaryWorkSnapshot {
            value_clones: value,
            refcounted_value_clones: value,
            string_allocations: value,
            array_handle_clones: value,
            cow_separations: value,
            reference_cell_creations: value,
            frame_allocations: value,
            frame_reuses: value,
            register_files_allocated: value,
            register_files_reused: value,
            internal_function_dispatches: value,
            symbol_map_lookups: value,
            symbol_linear_fallbacks: value,
            symbol_intern_hits: value,
            symbol_intern_misses: value,
            string_hash_cache_hits: value,
            string_hash_cache_misses: value,
            symbol_eq_fast_hits: value,
            symbol_eq_byte_fallbacks: value,
            array_dim_fetches: value,
            numeric_string_classify_calls: value,
            object_allocations: value,
            property_accesses: value,
            includes: value,
            autoloads: value,
        }
    }

    fn sample(
        nanos: u64,
        rich: u64,
        dense: u64,
        total_work: u64,
        children: &[RequestProfileSample],
    ) -> RequestProfileSample {
        let mut frame = RequestProfileFrame::default();
        for child in children {
            frame.absorb_sample(child);
        }
        reconcile_boundary_sample(
            nanos,
            0,
            0,
            BoundaryWorkSnapshot::default(),
            rich,
            dense,
            work(total_work),
            frame,
        )
    }

    fn assert_reconciles(parent: &RequestProfileSample, children: &[RequestProfileSample]) {
        assert_eq!(
            parent.exclusive_nanos
                + children
                    .iter()
                    .map(|child| child.inclusive_nanos)
                    .sum::<u64>(),
            parent.inclusive_nanos
        );
        assert_eq!(
            parent.exclusive_rich_instructions
                + children
                    .iter()
                    .map(|child| child.inclusive_rich_instructions)
                    .sum::<u64>(),
            parent.inclusive_rich_instructions
        );
        assert_eq!(
            parent.exclusive_dense_instructions
                + children
                    .iter()
                    .map(|child| child.inclusive_dense_instructions)
                    .sum::<u64>(),
            parent.inclusive_dense_instructions
        );
        let mut reconciled_work = parent.exclusive_work;
        for child in children {
            reconciled_work.saturating_add_assign(child.inclusive_work);
        }
        assert_eq!(reconciled_work, parent.inclusive_work);
    }

    #[test]
    fn request_profile_function_to_function_reconciles() {
        let child = sample(20, 4, 2, 3, &[]);
        let parent = sample(50, 10, 8, 9, std::slice::from_ref(&child));
        assert_reconciles(&parent, &[child]);
    }

    #[test]
    fn request_profile_method_to_builtin_reconciles() {
        let builtin = sample(8, 0, 0, 2, &[]);
        let method = sample(30, 12, 4, 7, std::slice::from_ref(&builtin));
        assert_reconciles(&method, &[builtin]);
    }

    #[test]
    fn request_profile_include_function_builtin_reconciles() {
        let builtin = sample(5, 1, 0, 1, &[]);
        let function = sample(15, 4, 2, 4, std::slice::from_ref(&builtin));
        assert_reconciles(&function, std::slice::from_ref(&builtin));
        let include = sample(40, 9, 6, 10, std::slice::from_ref(&function));
        assert_reconciles(&include, &[function]);
    }

    #[test]
    fn request_profile_recursive_function_reconciles_each_level() {
        let leaf = sample(7, 2, 1, 2, &[]);
        let recursive = sample(18, 6, 3, 5, std::slice::from_ref(&leaf));
        assert_reconciles(&recursive, &[leaf]);
    }

    #[test]
    fn request_profile_throwing_call_still_finishes_boundary() {
        let result: Result<(), &str> = Err("throw");
        let throwing = sample(11, 3, 1, 4, &[]);
        assert!(result.is_err());
        assert_eq!(throwing.exclusive_nanos, throwing.inclusive_nanos);
        assert_eq!(throwing.exclusive_work, throwing.inclusive_work);
    }

    #[test]
    fn request_profile_discard_preserves_only_profiled_children() {
        let nested = sample(4, 1, 1, 1, &[]);
        let mut discarded = RequestProfileFrame::default();
        discarded.absorb_sample(&nested);
        let mut parent_frame = RequestProfileFrame::default();
        parent_frame.absorb_profiled_children(discarded);
        let parent = reconcile_boundary_sample(
            20,
            0,
            0,
            BoundaryWorkSnapshot::default(),
            5,
            3,
            work(6),
            parent_frame,
        );
        assert_reconciles(&parent, &[nested]);
    }

    #[test]
    fn request_profile_native_decline_then_fallback_does_not_double_count() {
        let fallback = sample(9, 3, 2, 3, &[]);
        let mut declined = RequestProfileFrame::default();
        declined.absorb_sample(&fallback);
        let mut caller = RequestProfileFrame::default();
        caller.absorb_profiled_children(declined);
        let parent = reconcile_boundary_sample(
            15,
            0,
            0,
            BoundaryWorkSnapshot::default(),
            5,
            4,
            work(5),
            caller,
        );
        assert_reconciles(&parent, &[fallback]);
        assert_eq!(parent.exclusive_nanos, 6);
    }
}
