<?php

$label = match ($code) {
    200, 201 => "ok",
    404 => throw new RuntimeException("missing"),
    default => "other",
};

$fallback = $label ?: throw new RuntimeException("empty");
