<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-size-31dd57a8db area=builtin_contract kind=function symbol=shmop_size source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-size-31dd57a8db failure_category=builtin_contract requires_ref_extension=shmop
$name = "shmop_size";
echo function_exists($name) ? "available\n" : "missing\n";
