<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-reflectionenumunitcase-is-public-38b4d9fa7e area=internal_api_contract kind=class_constant symbol=ReflectionEnumUnitCase::IS_PUBLIC source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-reflectionenumunitcase-is-public-38b4d9fa7e failure_category=internal_api_contract requires_ref_extension=reflection
$class = "ReflectionEnumUnitCase";
$member = "IS_PUBLIC";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
