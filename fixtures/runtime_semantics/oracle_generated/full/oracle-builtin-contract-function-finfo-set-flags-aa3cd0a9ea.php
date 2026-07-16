<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-set-flags-aa3cd0a9ea area=builtin_contract kind=function symbol=finfo_set_flags source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-set-flags-aa3cd0a9ea failure_category=builtin_contract requires_ref_extension=fileinfo
try {
    $result = \finfo_set_flags();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
