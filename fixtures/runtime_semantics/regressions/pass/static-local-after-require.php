<?php
// runtime-semantics: expect=pass regression_category=statics reference_behavior=stdout:.min|.min regression_case=cross-unit-static-local-after-require

require __DIR__ . '/../_data/static-local-after-require-provider.php';

echo static_local_after_require(), '|', static_local_after_require(), "\n";
