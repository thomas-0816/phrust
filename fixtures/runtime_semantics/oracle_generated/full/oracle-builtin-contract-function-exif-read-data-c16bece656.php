<?php
// oracle-probe: id=oracle-builtin-contract-function-exif-read-data-c16bece656 area=builtin_contract kind=function symbol=exif_read_data source=ext/exif/exif.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-exif-read-data-c16bece656 failure_category=builtin_contract requires_ref_extension=exif
try {
    $result = \exif_read_data();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
