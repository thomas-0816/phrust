<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-file-2e01b8c8f2 area=builtin_contract kind=function symbol=finfo_file source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-file-2e01b8c8f2 failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \finfo_file();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
