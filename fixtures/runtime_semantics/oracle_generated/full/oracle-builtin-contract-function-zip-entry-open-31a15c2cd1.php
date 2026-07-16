<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-entry-open-31a15c2cd1 area=builtin_contract kind=function symbol=zip_entry_open source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-entry-open-31a15c2cd1 failure_category=builtin_contract requires_ref_extension=zip
$name = "zip_entry_open";
echo function_exists($name) ? "available\n" : "missing\n";
