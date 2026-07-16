<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-fetch-row-bb0c89981a area=builtin_contract kind=function symbol=pg_fetch_row source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-fetch-row-bb0c89981a failure_category=builtin_contract requires_ref_extension=pgsql
try {
    $result = \pg_fetch_row();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
