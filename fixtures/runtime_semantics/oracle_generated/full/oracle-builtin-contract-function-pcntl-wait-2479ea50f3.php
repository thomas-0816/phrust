<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-wait-2479ea50f3 area=builtin_contract kind=function symbol=pcntl_wait source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-wait-2479ea50f3 failure_category=builtin_contract requires_ref_extension=pcntl
try {
    $result = \pcntl_wait();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
