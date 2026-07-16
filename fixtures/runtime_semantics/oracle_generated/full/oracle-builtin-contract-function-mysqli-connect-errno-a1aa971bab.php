<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-connect-errno-a1aa971bab area=builtin_contract kind=function symbol=mysqli_connect_errno source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-connect-errno-a1aa971bab failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_connect_errno";
echo function_exists($name) ? "available\n" : "missing\n";
