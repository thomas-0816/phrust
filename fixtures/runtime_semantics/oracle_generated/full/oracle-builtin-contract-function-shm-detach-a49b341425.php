<?php
// oracle-probe: id=oracle-builtin-contract-function-shm-detach-a49b341425 area=builtin_contract kind=function symbol=shm_detach source=ext/sysvshm/sysvshm.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shm-detach-a49b341425 failure_category=builtin_contract requires_ref_extension=sysvshm
$name = "shm_detach";
echo function_exists($name) ? "available\n" : "missing\n";
