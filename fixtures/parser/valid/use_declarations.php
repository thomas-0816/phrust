<?php

namespace Imports;

use Vendor\Package\ClassName as Alias;
use function Vendor\Package\helper;
use const Vendor\Package\VALUE;
use Vendor\Package\{ClassA, ClassB as B};
use Vendor\Package\{function helper_two, const OTHER_VALUE};

echo Alias::class;
