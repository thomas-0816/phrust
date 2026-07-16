<?php
// oracle-probe: id=oracle-builtin-contract-function-grapheme-substr-b706ac4a91 area=builtin_contract kind=function symbol=grapheme_substr source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-grapheme-substr-b706ac4a91 failure_category=builtin_contract requires_ref_extension=intl
try {
    $result = \grapheme_substr();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
