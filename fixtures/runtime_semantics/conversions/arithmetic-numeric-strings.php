<?php
// runtime-semantics: category=conversions expect=pass
echo " 42" + 1, "|";
echo "42abc" + 1, "|";
echo "0.5x" + 1, "|";
echo +"0", "|";
echo -"0.0", "\n";
