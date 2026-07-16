<?php
// oracle-probe: id=oracle-internal-api-contract-class-datetimezone-87c74aa473 area=internal_api_contract kind=class symbol=DateTimeZone source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-datetimezone-87c74aa473 failure_category=internal_api_contract requires_ref_extension=date
$class = "DateTimeZone";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
