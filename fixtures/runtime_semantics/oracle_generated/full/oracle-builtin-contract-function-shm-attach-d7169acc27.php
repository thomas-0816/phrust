<?php
// oracle-probe: id=oracle-builtin-contract-function-shm-attach-d7169acc27 area=builtin_contract kind=function symbol=shm_attach source=ext/sysvshm/sysvshm.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shm-attach-d7169acc27 failure_category=builtin_contract requires_ref_extension=sysvshm
$name = "shm_attach";
echo function_exists($name) ? "available\n" : "missing\n";
