<?php
// oracle-probe: id=oracle-internal-api-contract-method-splobjectstorage-unserialize-6fe4358aeb area=internal_api_contract kind=method symbol=SplObjectStorage::__unserialize source=ext/spl/spl_observer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-splobjectstorage-unserialize-6fe4358aeb failure_category=internal_api_contract requires_ref_extension=spl
$class = "SplObjectStorage";
$member = "__unserialize";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
