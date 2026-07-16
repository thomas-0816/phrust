<?php
// oracle-probe: id=oracle-builtin-contract-function-http-response-code-5cbcebd05d area=builtin_contract kind=function symbol=http_response_code source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-http-response-code-5cbcebd05d failure_category=builtin_contract
try {
    $result = \http_response_code(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
