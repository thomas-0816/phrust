<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-ziparchive-opsys-os-2-8041624164 area=internal_api_contract kind=class_constant symbol=ZipArchive::OPSYS_OS_2 source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-ziparchive-opsys-os-2-8041624164 failure_category=internal_api_contract requires_ref_extension=zip
$class = "ZipArchive";
$member = "OPSYS_OS_2";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
