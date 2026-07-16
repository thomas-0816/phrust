<?php

function external_exit_function(): void
{
    echo "before-exit\n";
    exit(0);
}
