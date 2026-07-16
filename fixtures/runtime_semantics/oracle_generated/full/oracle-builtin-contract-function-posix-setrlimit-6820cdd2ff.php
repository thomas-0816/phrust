<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-setrlimit-6820cdd2ff area=builtin_contract kind=function symbol=posix_setrlimit source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-setrlimit-6820cdd2ff failure_category=builtin_contract requires_ref_extension=posix
try {
    $result = \posix_setrlimit();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
