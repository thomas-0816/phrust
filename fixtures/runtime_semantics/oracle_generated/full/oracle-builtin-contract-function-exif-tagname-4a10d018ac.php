<?php
// oracle-probe: id=oracle-builtin-contract-function-exif-tagname-4a10d018ac area=builtin_contract kind=function symbol=exif_tagname source=ext/exif/exif.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-exif-tagname-4a10d018ac failure_category=builtin_contract requires_ref_extension=exif
$name = "exif_tagname";
echo function_exists($name) ? "available\n" : "missing\n";
