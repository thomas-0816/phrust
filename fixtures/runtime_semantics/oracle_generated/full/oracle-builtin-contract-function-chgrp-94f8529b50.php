<?php
// oracle-probe: id=oracle-builtin-contract-function-chgrp-94f8529b50 area=builtin_contract kind=function symbol=chgrp source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chgrp-94f8529b50 failure_category=builtin_contract
try {
    $result = \chgrp();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
