<?php
// oracle-probe: id=oracle-internal-api-contract-method-valueerror-getprevious-0e5e533d7d area=internal_api_contract kind=method symbol=ValueError::getPrevious source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-valueerror-getprevious-0e5e533d7d failure_category=internal_api_contract
$class = "ValueError";
$member = "getPrevious";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
