<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-getlogin-80ff03b03f area=builtin_contract kind=function symbol=posix_getlogin source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-getlogin-80ff03b03f failure_category=builtin_contract requires_ref_extension=posix
try {
    $result = \posix_getlogin(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
