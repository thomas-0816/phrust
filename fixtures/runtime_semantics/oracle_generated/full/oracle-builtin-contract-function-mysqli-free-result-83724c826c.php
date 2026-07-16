<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-free-result-83724c826c area=builtin_contract kind=function symbol=mysqli_free_result source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-free-result-83724c826c failure_category=builtin_contract requires_ref_extension=mysqli
$name = "mysqli_free_result";
echo function_exists($name) ? "available\n" : "missing\n";
