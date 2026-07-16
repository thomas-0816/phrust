<?php
// oracle-probe: id=oracle-internal-api-contract-method-random-randomexception-getfile-42ea9c2cbd area=internal_api_contract kind=method symbol=Random\RandomException::getFile source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-random-randomexception-getfile-42ea9c2cbd failure_category=internal_api_contract requires_ref_extension=random
$class = "Random\\RandomException";
$member = "getFile";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
