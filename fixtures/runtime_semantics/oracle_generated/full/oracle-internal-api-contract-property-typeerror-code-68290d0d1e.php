<?php
// oracle-probe: id=oracle-internal-api-contract-property-typeerror-code-68290d0d1e area=internal_api_contract kind=property symbol=TypeError::code source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-typeerror-code-68290d0d1e failure_category=internal_api_contract
$class = "TypeError";
$member = "code";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
