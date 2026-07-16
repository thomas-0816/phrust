<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-resolve-include-path-8e0294b5c6 area=builtin_contract kind=function symbol=stream_resolve_include_path source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-resolve-include-path-8e0294b5c6 failure_category=builtin_contract
try {
    $result = \stream_resolve_include_path();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
