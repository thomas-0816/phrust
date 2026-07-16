<?php
// oracle-probe: id=oracle-builtin-contract-function-mt-rand-86a1e3177a area=builtin_contract kind=function symbol=mt_rand source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mt-rand-86a1e3177a failure_category=builtin_contract requires_ref_extension=random
$name = "mt_rand";
echo function_exists($name) ? "available\n" : "missing\n";
