<?php
// oracle-probe: id=oracle-internal-api-contract-method-phar-compress-ecf206c5f1 area=internal_api_contract kind=method symbol=Phar::compress source=ext/phar/phar_object.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-phar-compress-ecf206c5f1 failure_category=internal_api_contract requires_ref_extension=phar
$class = "Phar";
$member = "compress";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
