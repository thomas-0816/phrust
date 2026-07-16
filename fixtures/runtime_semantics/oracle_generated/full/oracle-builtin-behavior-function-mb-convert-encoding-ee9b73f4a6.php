<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-convert-encoding-ee9b73f4a6 area=builtin_behavior kind=function symbol=mb_convert_encoding source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-convert-encoding-ee9b73f4a6 failure_category=builtin_behavior requires_ref_extension=mbstring
try {
    $result = \mb_convert_encoding(string: [], to_encoding: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
