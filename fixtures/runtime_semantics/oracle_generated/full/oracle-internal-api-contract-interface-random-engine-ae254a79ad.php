<?php
// oracle-probe: id=oracle-internal-api-contract-interface-random-engine-ae254a79ad area=internal_api_contract kind=interface symbol=Random\Engine source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-interface-random-engine-ae254a79ad failure_category=internal_api_contract requires_ref_extension=random
$class = "Random\\Engine";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
