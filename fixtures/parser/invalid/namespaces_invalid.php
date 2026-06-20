<?php
// invalid: group use declaration is missing a closing brace

namespace Broken;

use Vendor\Package\{ClassA, ClassB;

echo "after";
