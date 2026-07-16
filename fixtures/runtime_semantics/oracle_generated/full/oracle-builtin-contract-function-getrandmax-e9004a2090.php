<?php
// oracle-probe: id=oracle-builtin-contract-function-getrandmax-e9004a2090 area=builtin_contract kind=function symbol=getrandmax source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-getrandmax-e9004a2090 failure_category=builtin_contract requires_ref_extension=random
$name = "getrandmax";
echo function_exists($name) ? "available\n" : "missing\n";
