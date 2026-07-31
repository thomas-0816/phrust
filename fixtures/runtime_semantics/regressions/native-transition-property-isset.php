<?php
// runtime-semantics: expect=pass regression_category=objects reference_behavior=stdout:first_|first_ regression_case=native-transition-property-isset

require __DIR__ . '/../_data/native-transition-property-state.php';

$state = new NativeTransitionPropertyState();
echo $state->setPrefix('first_'), "\n";
echo $state->setPrefix('second_'), "\n";
