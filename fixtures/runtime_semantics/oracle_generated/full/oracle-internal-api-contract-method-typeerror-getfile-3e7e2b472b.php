<?php
// oracle-probe: id=oracle-internal-api-contract-method-typeerror-getfile-3e7e2b472b area=internal_api_contract kind=method symbol=TypeError::getFile source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-typeerror-getfile-3e7e2b472b failure_category=internal_api_contract
$class = "TypeError";
$member = "getFile";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
