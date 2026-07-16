<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-fork-7f603eda8e area=builtin_contract kind=function symbol=pcntl_fork source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-fork-7f603eda8e failure_category=builtin_contract requires_ref_extension=pcntl
try {
    $result = \pcntl_fork(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
