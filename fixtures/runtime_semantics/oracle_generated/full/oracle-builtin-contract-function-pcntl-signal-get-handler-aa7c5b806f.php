<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-signal-get-handler-aa7c5b806f area=builtin_contract kind=function symbol=pcntl_signal_get_handler source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-signal-get-handler-aa7c5b806f failure_category=builtin_contract requires_ref_extension=pcntl
try {
    $result = \pcntl_signal_get_handler();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
