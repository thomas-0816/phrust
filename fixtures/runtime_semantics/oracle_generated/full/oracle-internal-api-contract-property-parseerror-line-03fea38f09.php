<?php
// oracle-probe: id=oracle-internal-api-contract-property-parseerror-line-03fea38f09 area=internal_api_contract kind=property symbol=ParseError::line source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-parseerror-line-03fea38f09 failure_category=internal_api_contract
$class = "ParseError";
$member = "line";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
