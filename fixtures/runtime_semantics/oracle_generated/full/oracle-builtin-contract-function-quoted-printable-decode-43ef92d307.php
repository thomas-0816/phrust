<?php
// oracle-probe: id=oracle-builtin-contract-function-quoted-printable-decode-43ef92d307 area=builtin_contract kind=function symbol=quoted_printable_decode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-quoted-printable-decode-43ef92d307 failure_category=builtin_contract
try {
    $result = \quoted_printable_decode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
