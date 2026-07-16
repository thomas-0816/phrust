<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-add-history-357b7e0d30 area=builtin_contract kind=function symbol=readline_add_history source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-add-history-357b7e0d30 failure_category=builtin_contract requires_ref_extension=readline
try {
    $result = \readline_add_history();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
