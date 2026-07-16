<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-fetch-assoc-3897b0c37b area=builtin_contract kind=function symbol=mysqli_fetch_assoc source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-fetch-assoc-3897b0c37b failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_fetch_assoc";
echo function_exists($name) ? "available\n" : "missing\n";
