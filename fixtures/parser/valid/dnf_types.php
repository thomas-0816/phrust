<?php

function intersections(A&B $both, (C&D)|E $dnf): (F&G)|H {
    echo $both;
}

$closure = function ((Reader&Writer)|null $stream): (Seekable&Readable)|false {
    echo $stream;
};

$arrow = fn((Left&Right)|Center $value): (OutA&OutB)|OutC => $value;
