<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-real-connect-29f4b5bbe7 area=builtin_contract kind=function symbol=mysqli_real_connect source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-real-connect-29f4b5bbe7 failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_real_connect";
echo function_exists($name) ? "available\n" : "missing\n";
