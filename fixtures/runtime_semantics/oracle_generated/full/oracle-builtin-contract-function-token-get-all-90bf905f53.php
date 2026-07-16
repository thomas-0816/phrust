<?php
// oracle-probe: id=oracle-builtin-contract-function-token-get-all-90bf905f53 area=builtin_contract kind=function symbol=token_get_all source=ext/tokenizer/tokenizer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-token-get-all-90bf905f53 failure_category=builtin_contract requires_ref_extension=tokenizer
try {
    $result = \token_get_all();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
