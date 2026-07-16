<?php
// oracle-probe: id=oracle-builtin-contract-function-zip-open-1adc1f8b25 area=builtin_contract kind=function symbol=zip_open source=ext/zip/php_zip.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zip-open-1adc1f8b25 failure_category=builtin_contract requires_ref_extension=zip
try {
    $result = \zip_open();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
