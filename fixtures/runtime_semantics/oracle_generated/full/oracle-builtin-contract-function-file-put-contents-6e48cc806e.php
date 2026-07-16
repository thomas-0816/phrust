<?php
// oracle-probe: id=oracle-builtin-contract-function-file-put-contents-6e48cc806e area=builtin_contract kind=function symbol=file_put_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-file-put-contents-6e48cc806e failure_category=builtin_contract
try {
    $result = \file_put_contents();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
