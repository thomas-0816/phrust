<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-isatty-5b3bcca16a area=builtin_contract kind=function symbol=stream_isatty source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-isatty-5b3bcca16a failure_category=builtin_contract
try {
    $result = \stream_isatty();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
