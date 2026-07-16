<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-num-fields-63b6b313c9 area=builtin_contract kind=function symbol=pg_num_fields source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-num-fields-63b6b313c9 failure_category=builtin_contract requires_ref_extension=pgsql
try {
    $result = \pg_num_fields();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
