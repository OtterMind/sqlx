#!/usr/bin/env python3
"""Read the printed result contract.

The CLI and the MCP server answer with one JSON object that holds one item per executed
statement: `results[].cols` for the columns, `rows` for the previewed rows and `count` for the
row count. A result set that did not fit the preview also carries `file`. Failures appear as an
`error` on the statement item, or as a top-level `error` when the batch never reached a statement.
"""
import json
import subprocess


def run(cli, data, *args, payload=None, workers=None, success=True, timeout=180):
    """Run the CLI and return its printed object."""
    command = [str(cli), '--data-dir', str(data)]
    if workers is not None:
        command += ['--worker-dir', str(workers)]
    command += list(args)
    result = subprocess.run(
        command,
        input=None if payload is None else json.dumps(payload),
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    try:
        value = json.loads(result.stdout)
    except ValueError:
        raise AssertionError(
            f'invalid CLI JSON; exit={result.returncode}; stdout={result.stdout[:400]}; stderr={result.stderr[-400:]}'
        ) from None
    if success:
        assert result.returncode == 0 and value['success'], value
    else:
        assert result.returncode != 0 and not value['success'], value
    return value


def items(value):
    """Every result item of one execution, in the order the statements ran."""
    return value.get('results', [])


def item(value, statement=0, result=0):
    """The item of one statement and result set, or None when it produced none."""
    for entry in items(value):
        if entry.get('stmt') == statement and entry.get('set', 0) == result:
            return entry
    return None


def rows(value, statement=0, result=0):
    """The previewed rows of one result set."""
    entry = item(value, statement, result)
    return [] if entry is None else entry.get('rows', [])


def columns(value, statement=0, result=0):
    """The columns of one result set, each as `[name, type]`."""
    entry = item(value, statement, result)
    return [] if entry is None else entry.get('cols', [])


def count(value, statement=0, result=0):
    """The row count of one result set, as the driver reported it."""
    entry = item(value, statement, result)
    return None if entry is None else entry.get('count')


def affected(value, statement=0, result=0):
    """The affected-row count of one statement, when the driver reported one."""
    entry = item(value, statement, result)
    return None if entry is None else entry.get('affected')


def error(value, statement=None):
    """The error of one statement, or the batch-level error."""
    if statement is None:
        return value.get('error')
    entry = item(value, statement)
    return None if entry is None else entry.get('error')


def skipped(value):
    """Statement indexes that never ran."""
    return value.get('skipped', [])


def stored_file(value, statement=0, result=0):
    """The absolute file of a stored result set, or None when it was printed completely."""
    entry = item(value, statement, result)
    return None if entry is None else entry.get('file')


def stored_rows(path):
    """Read a stored JSONL result file into a list of row arrays."""
    with open(path, encoding='utf-8') as handle:
        return [json.loads(line) for line in handle if line.strip()]
