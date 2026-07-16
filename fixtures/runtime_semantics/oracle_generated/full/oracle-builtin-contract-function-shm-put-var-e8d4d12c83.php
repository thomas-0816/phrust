<?php
// oracle-probe: id=oracle-builtin-contract-function-shm-put-var-e8d4d12c83 area=builtin_contract kind=function symbol=shm_put_var source=ext/sysvshm/sysvshm.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shm-put-var-e8d4d12c83 failure_category=builtin_contract requires_ref_extension=sysvshm
$name = "shm_put_var";
echo function_exists($name) ? "available\n" : "missing\n";
