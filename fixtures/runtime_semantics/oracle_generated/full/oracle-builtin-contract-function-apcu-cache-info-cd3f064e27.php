<?php
// oracle-probe: id=oracle-builtin-contract-function-apcu-cache-info-cd3f064e27 area=builtin_contract kind=function symbol=apcu_cache_info source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-apcu-cache-info-cd3f064e27 failure_category=builtin_contract requires_ref_extension=apcu
$name = "apcu_cache_info";
echo function_exists($name) ? "available\n" : "missing\n";
