<?php
// oracle-probe: id=oracle-builtin-contract-function-htmlspecialchars-decode-054cb5a4b3 area=builtin_contract kind=function symbol=htmlspecialchars_decode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-htmlspecialchars-decode-054cb5a4b3 failure_category=builtin_contract
try {
    $result = \htmlspecialchars_decode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
