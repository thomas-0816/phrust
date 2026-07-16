<?php
// oracle-probe: id=oracle-internal-api-contract-property-recursiveregexiterator-replacement-5cf71f790e area=internal_api_contract kind=property symbol=RecursiveRegexIterator::replacement source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-recursiveregexiterator-replacement-5cf71f790e failure_category=internal_api_contract requires_ref_extension=spl
$class = "RecursiveRegexIterator";
$member = "replacement";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
