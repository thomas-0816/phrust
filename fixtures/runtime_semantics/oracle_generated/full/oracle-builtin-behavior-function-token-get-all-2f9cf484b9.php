<?php
// oracle-probe: id=oracle-builtin-behavior-function-token-get-all-2f9cf484b9 area=builtin_behavior kind=function symbol=token_get_all source=ext/tokenizer/tokenizer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-token-get-all-2f9cf484b9 failure_category=builtin_behavior requires_ref_extension=tokenizer
try {
    $result = \token_get_all("", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
