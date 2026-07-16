<?php
// oracle-probe: id=oracle-builtin-contract-function-headers-sent-5a58a846f3 area=builtin_contract kind=function symbol=headers_sent source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-headers-sent-5a58a846f3 failure_category=builtin_contract
try {
    $result = \headers_sent(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
