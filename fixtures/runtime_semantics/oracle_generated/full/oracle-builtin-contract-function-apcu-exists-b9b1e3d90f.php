<?php
// oracle-probe: id=oracle-builtin-contract-function-apcu-exists-b9b1e3d90f area=builtin_contract kind=function symbol=apcu_exists source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-apcu-exists-b9b1e3d90f failure_category=builtin_contract requires_ref_extension=apcu
$name = "apcu_exists";
echo function_exists($name) ? "available\n" : "missing\n";
