<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-prepare-dbd98f2174 area=builtin_contract kind=function symbol=mysqli_prepare source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-prepare-dbd98f2174 failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_prepare";
echo function_exists($name) ? "available\n" : "missing\n";
