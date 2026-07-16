<?php
// oracle-probe: id=oracle-builtin-contract-function-apcu-add-77a9a5743b area=builtin_contract kind=function symbol=apcu_add source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-apcu-add-77a9a5743b failure_category=builtin_contract requires_ref_extension=apcu
$name = "apcu_add";
echo function_exists($name) ? "available\n" : "missing\n";
