<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-get-contents-7a444ec77f area=builtin_contract kind=function symbol=stream_get_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-get-contents-7a444ec77f failure_category=builtin_contract
try {
    $result = \stream_get_contents();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
