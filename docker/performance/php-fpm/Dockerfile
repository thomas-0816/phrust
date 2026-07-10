ARG PHP_VERSION=8.5.7
FROM php:${PHP_VERSION}-fpm-bookworm

RUN set -eux; \
    docker-php-ext-install -j1 mysqli

RUN set -eux; \
    php -r 'exit(PHP_VERSION === "8.5.7" ? 0 : 1);'; \
    php -r 'exit(extension_loaded("Zend OPcache") ? 0 : 1);'; \
    php -r 'exit(extension_loaded("mysqli") ? 0 : 1);'
