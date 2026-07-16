<?php
// oracle-probe: id=oracle-builtin-contract-function-random-int-66a36d23d8 area=builtin_contract kind=function symbol=random_int source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-random-int-66a36d23d8 failure_category=builtin_contract requires_ref_extension=random
$name = "random_int";
echo function_exists($name) ? "available\n" : "missing\n";
