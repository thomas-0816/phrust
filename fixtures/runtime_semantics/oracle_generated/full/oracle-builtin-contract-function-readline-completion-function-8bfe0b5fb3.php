<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-completion-function-8bfe0b5fb3 area=builtin_contract kind=function symbol=readline_completion_function source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-completion-function-8bfe0b5fb3 failure_category=builtin_contract requires_ref_extension=readline
try {
    $result = \readline_completion_function();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
