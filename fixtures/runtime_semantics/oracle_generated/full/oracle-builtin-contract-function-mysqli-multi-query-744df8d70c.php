<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-multi-query-744df8d70c area=builtin_contract kind=function symbol=mysqli_multi_query source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-multi-query-744df8d70c failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_multi_query";
echo function_exists($name) ? "available\n" : "missing\n";
