<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-rollback-abd769645c area=builtin_contract kind=function symbol=mysqli_rollback source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-rollback-abd769645c failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_rollback";
echo function_exists($name) ? "available\n" : "missing\n";
