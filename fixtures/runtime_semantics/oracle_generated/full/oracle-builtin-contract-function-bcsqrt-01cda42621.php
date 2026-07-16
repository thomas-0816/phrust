<?php
// oracle-probe: id=oracle-builtin-contract-function-bcsqrt-01cda42621 area=builtin_contract kind=function symbol=bcsqrt source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcsqrt-01cda42621 failure_category=builtin_contract requires_ref_extension=bcmath
$name = "bcsqrt";
echo function_exists($name) ? "available\n" : "missing\n";
