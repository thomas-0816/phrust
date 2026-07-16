<?php
// oracle-probe: id=oracle-builtin-contract-function-shm-remove-var-a67aa0bef9 area=builtin_contract kind=function symbol=shm_remove_var source=ext/sysvshm/sysvshm.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shm-remove-var-a67aa0bef9 failure_category=builtin_contract requires_ref_extension=sysvshm
$name = "shm_remove_var";
echo function_exists($name) ? "available\n" : "missing\n";
