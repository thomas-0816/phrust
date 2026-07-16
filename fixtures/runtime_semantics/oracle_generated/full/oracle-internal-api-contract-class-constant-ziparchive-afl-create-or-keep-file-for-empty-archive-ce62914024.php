<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-ziparchive-afl-create-or-keep-file-for-empty-archive-ce62914024 area=internal_api_contract kind=class_constant symbol=ZipArchive::AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-ziparchive-afl-create-or-keep-file-for-empty-archive-ce62914024 failure_category=internal_api_contract requires_ref_extension=zip
$class = "ZipArchive";
$member = "AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
