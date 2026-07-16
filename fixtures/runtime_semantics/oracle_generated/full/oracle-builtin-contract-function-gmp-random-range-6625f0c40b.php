<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-random-range-6625f0c40b area=builtin_contract kind=function symbol=gmp_random_range source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-random-range-6625f0c40b failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_random_range";
echo function_exists($name) ? "available\n" : "missing\n";
