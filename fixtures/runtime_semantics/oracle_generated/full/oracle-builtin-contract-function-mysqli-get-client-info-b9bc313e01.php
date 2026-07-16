<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-get-client-info-b9bc313e01 area=builtin_contract kind=function symbol=mysqli_get_client_info source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-get-client-info-b9bc313e01 failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_get_client_info";
echo function_exists($name) ? "available\n" : "missing\n";
