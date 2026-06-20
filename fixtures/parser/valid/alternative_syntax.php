<?php

if ($ready):
    echo "ready";
elseif ($other):
    echo "other";
else:
    echo "none";
endif;

while ($count < 2):
    $count = $count + 1;
endwhile;

for ($i = 0; $i < 2; $i = $i + 1):
    echo $i;
endfor;

foreach ([1, 2] as $item):
    echo $item;
endforeach;

switch ($item):
    case 1:
        echo "one";
        break;
    default:
        echo "other";
endswitch;
