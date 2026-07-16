<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-strtolower-2ddb8b376e area=builtin_behavior kind=function symbol=mb_strtolower source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-strtolower-2ddb8b376e failure_category=builtin_behavior requires_ref_extension=mbstring
try {
    $result = \mb_strtolower(string: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
