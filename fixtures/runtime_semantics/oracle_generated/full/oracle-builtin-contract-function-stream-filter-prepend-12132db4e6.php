<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-filter-prepend-12132db4e6 area=builtin_contract kind=function symbol=stream_filter_prepend source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-filter-prepend-12132db4e6 failure_category=builtin_contract
try {
    $result = \stream_filter_prepend();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
