<?php
// oracle-probe: id=oracle-builtin-contract-function-exif-imagetype-14ba15b6eb area=builtin_contract kind=function symbol=exif_imagetype source=ext/exif/exif.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-exif-imagetype-14ba15b6eb failure_category=builtin_contract requires_ref_extension=exif
try {
    $result = \exif_imagetype();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
