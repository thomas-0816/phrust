<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-read-b2f789f991 area=builtin_contract kind=function symbol=zip_read source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-read-b2f789f991 failure_category=builtin_contract requires_ref_extension=zip
try {
    $result = \zip_read();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
