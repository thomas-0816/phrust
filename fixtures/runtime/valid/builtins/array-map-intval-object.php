<?php

class NumericObject {}

var_dump(array_map('intval', [new NumericObject()]));
