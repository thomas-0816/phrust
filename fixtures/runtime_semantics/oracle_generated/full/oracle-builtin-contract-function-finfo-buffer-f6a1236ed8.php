<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-buffer-f6a1236ed8 area=builtin_contract kind=function symbol=finfo_buffer source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-buffer-f6a1236ed8 failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \finfo_buffer();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
