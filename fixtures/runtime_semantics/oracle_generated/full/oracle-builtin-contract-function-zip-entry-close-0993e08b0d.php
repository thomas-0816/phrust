<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-entry-close-0993e08b0d area=builtin_contract kind=function symbol=zip_entry_close source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-entry-close-0993e08b0d failure_category=builtin_contract requires_ref_extension=zip
try {
    $result = \zip_entry_close();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
