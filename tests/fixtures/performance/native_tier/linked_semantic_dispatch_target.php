<?php

final class PerfLinkedSemanticDispatch
{
    public const VALUE = 'callee-unit';
}

function perf_linked_semantic_dispatch(): string
{
    return PerfLinkedSemanticDispatch::VALUE;
}
