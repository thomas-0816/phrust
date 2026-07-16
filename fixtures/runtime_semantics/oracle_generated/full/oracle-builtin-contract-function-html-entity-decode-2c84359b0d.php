<?php
// oracle-probe: id=oracle-builtin-contract-function-html-entity-decode-2c84359b0d area=builtin_contract kind=function symbol=html_entity_decode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-html-entity-decode-2c84359b0d failure_category=builtin_contract
try {
    $result = \html_entity_decode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
