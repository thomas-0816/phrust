<?php
// oracle-probe: id=oracle-builtin-contract-function-exif-thumbnail-971ec0302b area=builtin_contract kind=function symbol=exif_thumbnail source=ext/exif/exif.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-exif-thumbnail-971ec0302b failure_category=builtin_contract requires_ref_extension=exif
$name = "exif_thumbnail";
echo function_exists($name) ? "available\n" : "missing\n";
