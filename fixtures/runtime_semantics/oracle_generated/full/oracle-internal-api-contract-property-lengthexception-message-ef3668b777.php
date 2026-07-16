<?php
// oracle-probe: id=oracle-internal-api-contract-property-lengthexception-message-ef3668b777 area=internal_api_contract kind=property symbol=LengthException::message source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-lengthexception-message-ef3668b777 failure_category=internal_api_contract requires_ref_extension=spl
$class = "LengthException";
$member = "message";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
