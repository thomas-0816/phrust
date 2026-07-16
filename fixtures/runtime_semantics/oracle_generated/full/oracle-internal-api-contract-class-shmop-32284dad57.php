<?php
// oracle-probe: id=oracle-internal-api-contract-class-shmop-32284dad57 area=internal_api_contract kind=class symbol=Shmop source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-shmop-32284dad57 failure_category=internal_api_contract requires_ref_extension=shmop
$class = "Shmop";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
