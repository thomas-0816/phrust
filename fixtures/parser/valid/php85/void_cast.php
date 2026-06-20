<?php

#[\NoDiscard]
function php85_value(): int {
    return 1;
}

(void) php85_value();
(void) (php85_value() |> abs(...));
