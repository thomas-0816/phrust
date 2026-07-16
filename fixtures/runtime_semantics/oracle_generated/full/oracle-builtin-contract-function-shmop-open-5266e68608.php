<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-open-5266e68608 area=builtin_contract kind=function symbol=shmop_open source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-open-5266e68608 failure_category=builtin_contract requires_ref_extension=shmop
$name = "shmop_open";
echo function_exists($name) ? "available\n" : "missing\n";
