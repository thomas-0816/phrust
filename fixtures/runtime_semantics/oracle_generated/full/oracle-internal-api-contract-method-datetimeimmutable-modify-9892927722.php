<?php
// oracle-probe: id=oracle-internal-api-contract-method-datetimeimmutable-modify-9892927722 area=internal_api_contract kind=method symbol=DateTimeImmutable::modify source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-datetimeimmutable-modify-9892927722 failure_category=internal_api_contract requires_ref_extension=date
$class = "DateTimeImmutable";
$member = "modify";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
