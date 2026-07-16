<?php
// oracle-probe: id=oracle-internal-api-contract-method-intldatepatterngenerator-create-5ee263fc57 area=internal_api_contract kind=method symbol=IntlDatePatternGenerator::create source=ext/intl/dateformat/datepatterngenerator.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-intldatepatterngenerator-create-5ee263fc57 failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlDatePatternGenerator";
$member = "create";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
