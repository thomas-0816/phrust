<?php
// oracle-probe: id=oracle-internal-api-contract-method-ziparchive-unchangeall-0c10195035 area=internal_api_contract kind=method symbol=ZipArchive::unchangeAll source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-ziparchive-unchangeall-0c10195035 failure_category=internal_api_contract requires_ref_extension=zip
$class = "ZipArchive";
$member = "unchangeAll";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
