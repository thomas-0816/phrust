<?php
// oracle-probe: id=oracle-internal-api-contract-property-closedgeneratorexception-message-97810b1585 area=internal_api_contract kind=property symbol=ClosedGeneratorException::message source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-closedgeneratorexception-message-97810b1585 failure_category=internal_api_contract
$class = "ClosedGeneratorException";
$member = "message";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
