impl NativeOptimizingAssumptions {
    fn generic_continuation_is_required(&self, continuation_id: u32) -> bool {
        self.generic_instruction_continuations
            .contains(&continuation_id)
    }

    fn reference_payload_proof(&self) -> Option<NativeReferencePayloadProof> {
        self.reference_payloads_are_proven
            .then_some(NativeReferencePayloadProof(()))
    }

    fn array_call_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_array_calls.contains(&continuation_id)
    }

    fn fixed_builtin_call_is_proven(&self, continuation_id: u32) -> bool {
        self.fixed_builtin_plans
            .get(&continuation_id)
            .is_some_and(NativeFixedBuiltinPublicationPlan::is_proven)
            || self.proven_fixed_builtin_calls.contains(&continuation_id)
    }

    fn throwable_method_call_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_throwable_method_calls
            .contains(&continuation_id)
    }

    fn array_instruction_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_array_instructions.contains(&continuation_id)
    }

    fn array_instruction_root_is_by_reference(&self, continuation_id: u32) -> bool {
        self.by_ref_array_instructions.contains(&continuation_id)
    }

    fn binary_instruction_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_binary_instructions.contains(&continuation_id)
    }

    fn binary_instruction_operand_classes(
        &self,
        continuation_id: u32,
    ) -> Option<(SsaValueClass, SsaValueClass)> {
        self.binary_operand_classes.get(&continuation_id).copied()
    }

    fn numeric_binary_instruction_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_numeric_binary_instructions
            .contains(&continuation_id)
    }

    fn scalar_control_instruction_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_scalar_control_instructions
            .contains(&continuation_id)
    }

    fn array_instruction_is_fresh(&self, continuation_id: u32) -> bool {
        self.fresh_array_instructions.contains(&continuation_id)
    }

    fn fresh_unique_array_key(&self, continuation_id: u32) -> Option<Option<i64>> {
        self.fresh_unique_array_keys
            .get(&continuation_id)
            .copied()
    }

    fn fresh_array_capacity(&self, continuation_id: u32) -> Option<u32> {
        self.fresh_array_capacities.get(&continuation_id).copied()
    }

    fn local_load_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_local_loads.contains(&continuation_id)
    }

    fn request_local_store_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_request_local_stores.contains(&continuation_id)
    }

    fn return_reference_store_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_return_reference_stores
            .contains(&continuation_id)
    }

    fn return_reference_is_prebound(&self, local: LocalId) -> bool {
        self.proven_reference_locals.contains(&local)
    }

    fn terminator_is_proven(&self, continuation_id: u32) -> bool {
        self.proven_terminators.contains(&continuation_id)
    }

    fn return_plan(&self, continuation_id: u32) -> Option<NativeOptimizingReturnPlan> {
        self.return_plans.get(&continuation_id).copied()
    }
}
