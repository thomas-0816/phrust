<?php
// oracle-probe: id=oracle-builtin-contract-function-getimagesize-8d1be17dcc area=builtin_contract kind=function symbol=getimagesize source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getimagesize-8d1be17dcc failure_category=builtin_contract
try {
    $result = \getimagesize();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
