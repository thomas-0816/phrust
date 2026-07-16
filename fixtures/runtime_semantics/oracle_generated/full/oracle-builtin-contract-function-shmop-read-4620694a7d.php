<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-read-4620694a7d area=builtin_contract kind=function symbol=shmop_read source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-read-4620694a7d failure_category=builtin_contract requires_ref_extension=shmop
$name = "shmop_read";
echo function_exists($name) ? "available\n" : "missing\n";
