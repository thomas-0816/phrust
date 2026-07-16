<?php
// oracle-probe: id=oracle-internal-api-contract-method-ziparchive-statname-aa5a7e32d5 area=internal_api_contract kind=method symbol=ZipArchive::statName source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-ziparchive-statname-aa5a7e32d5 failure_category=internal_api_contract requires_ref_extension=zip
$class = "ZipArchive";
$member = "statName";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
