<?php
// runtime-semantics: expect=pass regression_category=runtime_dispatch reference_behavior=late_static_scope_uses_callback_target regression_case=cross_unit_late_static_callback

require __DIR__ . '/../_data/cross-unit-late-static-callback-outer.php';

echo (new OuterRenderFrame())->render(), "\n";
