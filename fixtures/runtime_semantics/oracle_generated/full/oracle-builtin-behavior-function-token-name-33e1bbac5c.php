<?php
// oracle-probe: id=oracle-builtin-behavior-function-token-name-33e1bbac5c area=builtin_behavior kind=function symbol=token_name source=ext/tokenizer/tokenizer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-token-name-33e1bbac5c failure_category=builtin_behavior requires_ref_extension=tokenizer
try {
    $result = \token_name(id: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
