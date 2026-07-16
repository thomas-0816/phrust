<?php
// oracle-probe: id=oracle-internal-api-contract-method-random-engine-xoshiro256starstar-construct-f23e791c0d area=internal_api_contract kind=method symbol=Random\Engine\Xoshiro256StarStar::__construct source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-random-engine-xoshiro256starstar-construct-f23e791c0d failure_category=internal_api_contract requires_ref_extension=random
$class = "Random\\Engine\\Xoshiro256StarStar";
$member = "__construct";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
