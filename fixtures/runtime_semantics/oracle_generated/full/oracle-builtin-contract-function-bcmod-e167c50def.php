<?php
// oracle-probe: id=oracle-builtin-contract-function-bcmod-e167c50def area=builtin_contract kind=function symbol=bcmod source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcmod-e167c50def failure_category=builtin_contract requires_ref_extension=bcmath
$name = "bcmod";
echo function_exists($name) ? "available\n" : "missing\n";
