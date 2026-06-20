<?php

try {
    throw new RuntimeException("problem");
} catch (RuntimeException|InvalidArgumentException $e) {
    echo $e->getMessage();
} catch (Throwable) {
    echo "throwable";
} finally {
    echo "done";
}
