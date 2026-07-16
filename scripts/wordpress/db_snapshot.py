#!/usr/bin/env python3
"""Create and restore deterministic logical snapshots for WordPress smoke DBs."""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from common import parse_mysql_dsn, repo_path


def quote_identifier(value: str) -> str:
    return "`" + value.replace("`", "``") + "`"


def mysql_command(connection: dict[str, object], *, database: bool) -> list[str]:
    mysql = shutil.which("mysql")
    if mysql is None:
        raise RuntimeError("mysql client is required for WordPress database snapshots")
    command = [
        mysql,
        "--batch",
        "--raw",
        "--skip-column-names",
        "--default-character-set=utf8mb4",
        f"--host={connection['host']}",
        f"--port={connection['port']}",
        f"--user={connection['user']}",
    ]
    if database:
        command.append(f"--database={connection['database']}")
    return command


def mysql_environment(connection: dict[str, object]) -> dict[str, str]:
    environment = os.environ.copy()
    password = str(connection["password"])
    if password:
        environment["MYSQL_PWD"] = password
    else:
        environment.pop("MYSQL_PWD", None)
    return environment


def query(connection: dict[str, object], sql: str) -> str:
    completed = subprocess.run(
        [*mysql_command(connection, database=True), f"--execute={sql}"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=mysql_environment(connection),
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or "mysql query failed")
    return completed.stdout


def list_values(connection: dict[str, object], sql: str) -> list[str]:
    return [line for line in query(connection, sql).splitlines() if line]


def dump_snapshot(connection: dict[str, object], output: Path) -> None:
    database = str(connection["database"])
    if not database:
        raise RuntimeError("snapshot DSN must name a database")
    tables = list_values(
        connection,
        "SELECT TABLE_NAME FROM information_schema.TABLES "
        "WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE' "
        "ORDER BY TABLE_NAME",
    )
    if not tables:
        raise RuntimeError(f"database {database!r} contains no tables")

    lines = [
        "-- Phrust deterministic WordPress database snapshot",
        "-- format: 2",
        "SET NAMES utf8mb4;",
        "SET SQL_MODE='NO_AUTO_VALUE_ON_ZERO';",
        "SET FOREIGN_KEY_CHECKS=0;",
        "",
    ]
    for table in tables:
        quoted_table = quote_identifier(table)
        create_output = query(connection, f"SHOW CREATE TABLE {quoted_table}")
        if "\t" not in create_output:
            raise RuntimeError(f"SHOW CREATE TABLE returned no definition for {table}")
        create_sql = create_output.split("\t", 1)[1].rstrip()
        columns = list_values(
            connection,
            "SELECT COLUMN_NAME FROM information_schema.COLUMNS "
            f"WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{table.replace(chr(39), chr(39) * 2)}' "
            "ORDER BY ORDINAL_POSITION",
        )
        if not columns:
            raise RuntimeError(f"table {table!r} contains no columns")
        primary = list_values(
            connection,
            "SELECT COLUMN_NAME FROM information_schema.STATISTICS "
            f"WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{table.replace(chr(39), chr(39) * 2)}' "
            "AND INDEX_NAME = 'PRIMARY' ORDER BY SEQ_IN_INDEX",
        )
        order = primary or columns
        expressions = ", ".join(
            f"IF({quote_identifier(column)} IS NULL, 'NULL', "
            f"HEX(CAST({quote_identifier(column)} AS BINARY)))"
            for column in columns
        )
        rows = query(
            connection,
            f"SELECT {expressions} FROM {quoted_table} ORDER BY "
            + ", ".join(quote_identifier(column) for column in order),
        )

        lines.extend((f"DROP TABLE IF EXISTS {quoted_table};", create_sql + ";"))
        quoted_columns = ", ".join(quote_identifier(column) for column in columns)
        for row in rows.splitlines():
            values = [
                "NULL" if value == "NULL" else f"UNHEX('{value}')"
                for value in row.split("\t")
            ]
            if len(values) != len(columns):
                raise RuntimeError(f"table {table!r} produced a malformed row")
            lines.append(
                f"INSERT INTO {quoted_table} ({quoted_columns}) VALUES ({', '.join(values)});"
            )
        lines.append("")
    lines.extend(("SET FOREIGN_KEY_CHECKS=1;", ""))

    output.parent.mkdir(parents=True, exist_ok=True)
    payload = "\n".join(lines).encode("utf-8")
    with tempfile.NamedTemporaryFile(dir=output.parent, delete=False) as handle:
        temporary = Path(handle.name)
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, output)


def restore_snapshot(connection: dict[str, object], source: Path) -> None:
    database = str(connection["database"])
    if not database:
        raise RuntimeError("snapshot DSN must name a database")
    payload = source.read_bytes()
    if not payload.startswith(b"-- Phrust deterministic WordPress database snapshot\n"):
        raise RuntimeError("input is not a Phrust WordPress database snapshot")
    if not payload.startswith(
        b"-- Phrust deterministic WordPress database snapshot\n-- format: 2\n"
    ):
        payload = upgrade_legacy_numeric_values(payload)
    quoted_database = quote_identifier(database)
    reset = subprocess.run(
        [
            *mysql_command(connection, database=False),
            f"--execute=DROP DATABASE IF EXISTS {quoted_database}; "
            f"CREATE DATABASE {quoted_database} CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=mysql_environment(connection),
        check=False,
    )
    if reset.returncode != 0:
        raise RuntimeError(reset.stderr.decode(errors="replace").strip())
    restored = subprocess.run(
        mysql_command(connection, database=True),
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=mysql_environment(connection),
        check=False,
    )
    if restored.returncode != 0:
        raise RuntimeError(restored.stderr.decode(errors="replace").strip())


def upgrade_legacy_numeric_values(payload: bytes) -> bytes:
    """Repair the unversioned development snapshot's numeric HEX encoding."""
    text = payload.decode("utf-8")
    numeric_columns: dict[str, set[str]] = {}
    current_table: str | None = None
    for line in text.splitlines():
        create = re.match(r"CREATE TABLE `([^`]+)`", line)
        if create:
            current_table = create.group(1)
            numeric_columns[current_table] = set()
            continue
        if current_table is None:
            continue
        if line.startswith(") ENGINE="):
            current_table = None
            continue
        column = re.match(
            r"\s+`([^`]+)`\s+(tinyint|smallint|mediumint|int|bigint|decimal|float|double|bit)\b",
            line,
            re.IGNORECASE,
        )
        if column:
            numeric_columns[current_table].add(column.group(1))

    converted = []
    insert_pattern = re.compile(
        r"INSERT INTO `([^`]+)` \(([^)]+)\) VALUES \((.*)\);"
    )
    value_pattern = re.compile(r"UNHEX\('([0-9A-F]*)'\)")
    for line in text.splitlines():
        insert = insert_pattern.fullmatch(line)
        if insert is None:
            converted.append(line)
            continue
        table = insert.group(1)
        columns = [part.strip().strip("`") for part in insert.group(2).split(",")]
        values = insert.group(3).split(", ")
        if len(columns) != len(values):
            raise RuntimeError(f"legacy snapshot row for {table!r} is malformed")
        for index, column in enumerate(columns):
            if column not in numeric_columns.get(table, set()):
                continue
            encoded = value_pattern.fullmatch(values[index])
            if encoded is None:
                continue
            raw = encoded.group(1)
            values[index] = str(int(raw, 16)) if raw else "0"
        converted.append(
            f"INSERT INTO `{table}` ({insert.group(2)}) VALUES ({', '.join(values)});"
        )
    return ("\n".join(converted) + "\n").encode("utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("dump", "restore"))
    parser.add_argument("path")
    parser.add_argument("--dsn-env", default="PHRUST_MYSQL_TEST_DSN")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    dsn = os.environ.get(args.dsn_env, "").strip()
    if not dsn:
        raise RuntimeError(f"{args.dsn_env} must be set")
    connection = parse_mysql_dsn(dsn)
    path = repo_path(args.path) or Path(args.path)
    if args.action == "dump":
        dump_snapshot(connection, path)
        print(f"[ok] wrote deterministic WordPress DB snapshot: {path}")
    else:
        restore_snapshot(connection, path)
        print(f"[ok] restored deterministic WordPress DB snapshot: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
