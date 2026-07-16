<?php
// oracle-probe: id=oracle-builtin-behavior-function-iconv-e392997f64 area=builtin_behavior kind=function symbol=iconv source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-iconv-e392997f64 failure_category=builtin_behavior requires_ref_extension=iconv
try {
    $result = \iconv(from_encoding: "", to_encoding: "", string: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
