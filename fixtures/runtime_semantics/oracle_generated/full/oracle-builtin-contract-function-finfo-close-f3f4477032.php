<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-close-f3f4477032 area=builtin_contract kind=function symbol=finfo_close source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-close-f3f4477032 failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \finfo_close();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
