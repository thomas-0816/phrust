<?php
// oracle-probe: id=oracle-builtin-contract-function-bcsub-70ebf94065 area=builtin_contract kind=function symbol=bcsub source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcsub-70ebf94065 failure_category=builtin_contract requires_ref_extension=bcmath
$name = "bcsub";
echo function_exists($name) ? "available\n" : "missing\n";
