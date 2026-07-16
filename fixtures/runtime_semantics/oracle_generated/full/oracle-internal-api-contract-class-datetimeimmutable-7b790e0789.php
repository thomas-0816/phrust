<?php
// oracle-probe: id=oracle-internal-api-contract-class-datetimeimmutable-7b790e0789 area=internal_api_contract kind=class symbol=DateTimeImmutable source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-datetimeimmutable-7b790e0789 failure_category=internal_api_contract requires_ref_extension=date
$class = "DateTimeImmutable";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
