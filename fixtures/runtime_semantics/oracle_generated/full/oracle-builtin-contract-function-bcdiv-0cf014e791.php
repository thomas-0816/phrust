<?php
// oracle-probe: id=oracle-builtin-contract-function-bcdiv-0cf014e791 area=builtin_contract kind=function symbol=bcdiv source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcdiv-0cf014e791 failure_category=builtin_contract requires_ref_extension=bcmath
$name = "bcdiv";
echo function_exists($name) ? "available\n" : "missing\n";
