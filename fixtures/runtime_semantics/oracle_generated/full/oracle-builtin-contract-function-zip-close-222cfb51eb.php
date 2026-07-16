<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-close-222cfb51eb area=builtin_contract kind=function symbol=zip_close source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-close-222cfb51eb failure_category=builtin_contract requires_ref_extension=zip
$name = "zip_close";
echo function_exists($name) ? "available\n" : "missing\n";
