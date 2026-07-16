use super::NativeExecutionContext;

#[derive(Default)]
pub(super) struct NativeRuntimeTelemetry {
    pub(super) counters: crate::counters::VmCounters,
    pub(super) helper_timing_stack: Vec<NativeHelperTimingFrame>,
}

pub(super) struct NativeHelperTimingFrame {
    helper_id: &'static str,
    started_at: std::time::Instant,
    pub(super) child_time_nanos: u64,
}

impl NativeRuntimeTelemetry {
    fn enter_helper(&mut self, helper_id: &'static str) {
        self.counters.runtime_helper_calls = self.counters.runtime_helper_calls.saturating_add(1);
        let count = self
            .counters
            .runtime_helper_calls_by_id
            .entry(helper_id.to_owned())
            .or_default();
        *count = count.saturating_add(1);
        self.helper_timing_stack.push(NativeHelperTimingFrame {
            helper_id,
            started_at: std::time::Instant::now(),
            child_time_nanos: 0,
        });
    }

    fn exit_helper(&mut self, helper_id: &'static str) {
        let Some(frame) = self.helper_timing_stack.pop() else {
            return;
        };
        debug_assert_eq!(frame.helper_id, helper_id);
        let elapsed = frame
            .started_at
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let exclusive = elapsed.saturating_sub(frame.child_time_nanos);
        self.counters.runtime_helper_time_nanos = self
            .counters
            .runtime_helper_time_nanos
            .saturating_add(exclusive);
        let helper_time = self
            .counters
            .runtime_helper_time_nanos_by_id
            .entry(helper_id.to_owned())
            .or_default();
        *helper_time = helper_time.saturating_add(exclusive);
        if let Some(parent) = self.helper_timing_stack.last_mut() {
            parent.child_time_nanos = parent.child_time_nanos.saturating_add(elapsed);
        }
    }
}

impl NativeExecutionContext<'_> {
    pub(in crate::vm) fn runtime_counters(&self) -> crate::counters::VmCounters {
        self.runtime_telemetry.borrow().counters.clone()
    }

    pub(super) fn enter_runtime_helper(&self, helper_id: &'static str) {
        self.runtime_telemetry.borrow_mut().enter_helper(helper_id);
    }

    pub(super) fn exit_runtime_helper(&self, helper_id: &'static str) {
        self.runtime_telemetry.borrow_mut().exit_helper(helper_id);
    }

    pub(super) fn merge_nested_runtime_counters(
        &self,
        nested: &crate::counters::VmCounters,
        nested_elapsed: std::time::Duration,
    ) {
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        let counters = &mut telemetry.counters;
        counters.native_execution_entries = counters
            .native_execution_entries
            .saturating_add(nested.native_execution_entries);
        counters.native_region_entries = counters
            .native_region_entries
            .saturating_add(nested.native_region_entries);
        counters.native_region_side_exits = counters
            .native_region_side_exits
            .saturating_add(nested.native_region_side_exits);
        counters.native_call_direct = counters
            .native_call_direct
            .saturating_add(nested.native_call_direct);
        counters.native_call_dynamic = counters
            .native_call_dynamic
            .saturating_add(nested.native_call_dynamic);
        counters.native_transition_count = counters
            .native_transition_count
            .saturating_add(nested.native_transition_count);
        counters.native_transition_time_nanos = counters
            .native_transition_time_nanos
            .saturating_add(nested.native_transition_time_nanos);
        counters.runtime_helper_calls = counters
            .runtime_helper_calls
            .saturating_add(nested.runtime_helper_calls);
        counters.runtime_helper_time_nanos = counters
            .runtime_helper_time_nanos
            .saturating_add(nested.runtime_helper_time_nanos);
        counters.runtime_helper_object_release_fast_paths = counters
            .runtime_helper_object_release_fast_paths
            .saturating_add(nested.runtime_helper_object_release_fast_paths);
        counters.runtime_helper_object_release_root_scans = counters
            .runtime_helper_object_release_root_scans
            .saturating_add(nested.runtime_helper_object_release_root_scans);
        counters.gc_safepoint_polls = counters
            .gc_safepoint_polls
            .saturating_add(nested.gc_safepoint_polls);
        counters.gc_safepoint_collections = counters
            .gc_safepoint_collections
            .saturating_add(nested.gc_safepoint_collections);
        merge_counter_map(
            &mut counters.native_region_side_exits_by_reason,
            &nested.native_region_side_exits_by_reason,
        );
        merge_counter_map(
            &mut counters.native_transition_by_reason,
            &nested.native_transition_by_reason,
        );
        merge_counter_map(
            &mut counters.native_transition_time_nanos_by_reason,
            &nested.native_transition_time_nanos_by_reason,
        );
        merge_counter_map(
            &mut counters.runtime_helper_calls_by_id,
            &nested.runtime_helper_calls_by_id,
        );
        merge_counter_map(
            &mut counters.runtime_helper_time_nanos_by_id,
            &nested.runtime_helper_time_nanos_by_id,
        );
        if let Some(parent) = telemetry.helper_timing_stack.last_mut() {
            let nested_elapsed = nested_elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
            parent.child_time_nanos = parent.child_time_nanos.saturating_add(nested_elapsed);
        }
    }

    pub(super) fn active_helper_child_time_nanos(&self) -> u64 {
        self.runtime_telemetry
            .borrow()
            .helper_timing_stack
            .last()
            .map_or(0, |frame| frame.child_time_nanos)
    }

    pub(super) fn record_object_release_root_check(&self, fast_path: bool) {
        if !self.options.collect_counters {
            return;
        }
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        if fast_path {
            telemetry.counters.runtime_helper_object_release_fast_paths = telemetry
                .counters
                .runtime_helper_object_release_fast_paths
                .saturating_add(1);
        } else {
            telemetry.counters.runtime_helper_object_release_root_scans = telemetry
                .counters
                .runtime_helper_object_release_root_scans
                .saturating_add(1);
        }
    }

    pub(super) fn record_native_transition(
        &self,
        reason: &'static str,
        elapsed: std::time::Duration,
        nested_helper_time_nanos: u64,
    ) {
        let elapsed_nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        telemetry.counters.native_transition_count =
            telemetry.counters.native_transition_count.saturating_add(1);
        telemetry.counters.native_transition_time_nanos = telemetry
            .counters
            .native_transition_time_nanos
            .saturating_add(elapsed_nanos);
        let count = telemetry
            .counters
            .native_transition_by_reason
            .entry(reason.to_owned())
            .or_default();
        *count = count.saturating_add(1);
        let time = telemetry
            .counters
            .native_transition_time_nanos_by_reason
            .entry(reason.to_owned())
            .or_default();
        *time = time.saturating_add(elapsed_nanos);
        if let Some(parent) = telemetry.helper_timing_stack.last_mut() {
            parent.child_time_nanos = parent
                .child_time_nanos
                .saturating_add(elapsed_nanos.saturating_sub(nested_helper_time_nanos));
        }
    }
}

fn merge_counter_map(
    target: &mut std::collections::BTreeMap<String, u64>,
    source: &std::collections::BTreeMap<String, u64>,
) {
    for (name, value) in source {
        let entry = target.entry(name.clone()).or_default();
        *entry = entry.saturating_add(*value);
    }
}
