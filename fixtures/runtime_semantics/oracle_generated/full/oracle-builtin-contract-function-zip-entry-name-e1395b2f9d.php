<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-entry-name-e1395b2f9d area=builtin_contract kind=function symbol=zip_entry_name source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-entry-name-e1395b2f9d failure_category=builtin_contract requires_ref_extension=zip
try {
    $result = \zip_entry_name();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
