<?php
// oracle-probe: id=oracle-internal-api-contract-interface-datetimeinterface-1e653c30ac area=internal_api_contract kind=interface symbol=DateTimeInterface source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-interface-datetimeinterface-1e653c30ac failure_category=internal_api_contract requires_ref_extension=date
$class = "DateTimeInterface";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
