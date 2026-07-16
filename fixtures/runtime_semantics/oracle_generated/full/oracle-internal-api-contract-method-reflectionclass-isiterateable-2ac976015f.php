<?php
// oracle-probe: id=oracle-internal-api-contract-method-reflectionclass-isiterateable-2ac976015f area=internal_api_contract kind=method symbol=ReflectionClass::isIterateable source=ext/reflection/php_reflection.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-reflectionclass-isiterateable-2ac976015f failure_category=internal_api_contract requires_ref_extension=reflection
$class = "ReflectionClass";
$member = "isIterateable";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
