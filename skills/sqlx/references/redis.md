# Redis operations

Each command below performs one operation. Submit it through `sqlx sql execute --datasource <id> --command "..."`. Repeat `--command` in the same invocation when commands need to share connection state; they run in order and the first error stops the rest.

A command is written the way `redis-cli` writes it: the command name followed by its arguments, separated by spaces, with single quotes, double quotes and backslash escapes available for arguments that contain spaces. Redis has no SQL, so a command is the statement here.

`--type redis` connects over the Redis protocol on port 6379 by default. `--username` and `--password` are sent with `AUTH`, `--database` selects the numeric keyspace (0 by default), and `--tls disable` uses `redis://` while any other value uses `rediss://`. Official links target the Redis command reference; match the connected server version when you look them up. The documentation root is https://redis.io/docs/latest/commands/.

## 1. Identify the current connection

**Purpose:** Check that the server answers and which keyspace the connection uses. No placeholders need replacement.

```text
PING
CLIENT INFO
DBSIZE
```

**Result:** `PING` answers `PONG` as one `value` row. `CLIENT INFO` returns one row per connection field, including the selected database. `DBSIZE` returns the number of keys as an `integer` row.

**Official documentation:** [PING](https://redis.io/docs/latest/commands/ping/) · [CLIENT INFO](https://redis.io/docs/latest/commands/client-info/) · [DBSIZE](https://redis.io/docs/latest/commands/dbsize/)

## 2. Read and write a string

**Purpose:** Read or write a single value. Replace `key` and `value`.

```text
SET key "value"
GET key
TTL key
```

**Result:** `SET` returns `OK`; add `EX <seconds>` to set an expiry. `GET` returns the `value` column, or a NULL row when the key does not exist. `TTL` returns the remaining seconds as an integer, `-1` without an expiry and `-2` for a missing key.

**Official documentation:** [SET](https://redis.io/docs/latest/commands/set/) · [GET](https://redis.io/docs/latest/commands/get/) · [TTL](https://redis.io/docs/latest/commands/ttl/)

## 3. Inspect a hash

**Purpose:** Read or write the fields of a hash. Replace `key` and the field names.

```text
HGETALL key
HSCAN key 0 COUNT 100
```

**Result:** `HGETALL` returns two columns, `field` and `value`, with one row per field. `HSCAN` returns a `c1`/`c2` row: the cursor and the field/value pairs as JSON text, which is what a nested reply looks like.

**Official documentation:** [HGETALL](https://redis.io/docs/latest/commands/hgetall/) · [HSCAN](https://redis.io/docs/latest/commands/hscan/)

## 4. Inspect a list, set or sorted set

**Purpose:** Read a range of a collection. Replace `key` and the indexes.

```text
LRANGE key 0 -1
SMEMBERS key
ZRANGE key 0 -1 WITHSCORES
```

**Result:** `LRANGE` and `SMEMBERS` return one row per element in the `value` column. `ZRANGE ... WITHSCORES` returns `field`/`value` rows, member and score alternating.

**Official documentation:** [LRANGE](https://redis.io/docs/latest/commands/lrange/) · [SMEMBERS](https://redis.io/docs/latest/commands/smembers/) · [ZRANGE](https://redis.io/docs/latest/commands/zrange/)

## 5. Find keys without blocking the server

**Purpose:** Walk the keyspace in small steps. Replace the cursor and pattern.

```text
SCAN 0 MATCH prefix:* COUNT 100
```

**Result:** One row with two columns: the next cursor and the matching keys as JSON text. Repeat with the returned cursor until it is `0`. Never use `KEYS` on a production keyspace, because it blocks the server while it walks every key.

**Official documentation:** [SCAN](https://redis.io/docs/latest/commands/scan/)

## 6. Check a key's type and size before reading it

**Purpose:** Avoid a type error and judge the cost of a read. Replace `key`.

```text
TYPE key
STRLEN key
HLEN key
LLEN key
```

**Result:** `TYPE` returns `string`, `hash`, `list`, `set`, `zset` or `none`. The size commands return an integer row, and they fail with `WRONGTYPE` when the key holds another type.

**Official documentation:** [TYPE](https://redis.io/docs/latest/commands/type/) · [STRLEN](https://redis.io/docs/latest/commands/strlen/) · [HLEN](https://redis.io/docs/latest/commands/hlen/) · [LLEN](https://redis.io/docs/latest/commands/llen/)

## 7. Delete keys

**Purpose:** Remove keys and report how many were removed. Replace `key`.

```text
DEL key
UNLINK key
```

**Result:** One `integer` row with the number of keys removed. `UNLINK` frees the memory in a background thread, which is preferable for large values. `FLUSHDB` and `FLUSHALL` remove every key in the keyspace or in the whole server and cannot be undone.

**Official documentation:** [DEL](https://redis.io/docs/latest/commands/del/) · [UNLINK](https://redis.io/docs/latest/commands/unlink/)

## 8. Inspect server state

**Purpose:** Read runtime information without changing the server. No placeholders need replacement.

```text
INFO server
CONFIG GET maxmemory
SLOWLOG GET 10
```

**Result:** `INFO` returns one row whose `value` holds the requested section as text. `CONFIG GET` returns `field`/`value` rows. `SLOWLOG GET` returns one row per entry, with the entry fields as JSON text.

**Official documentation:** [INFO](https://redis.io/docs/latest/commands/info/) · [CONFIG GET](https://redis.io/docs/latest/commands/config-get/) · [SLOWLOG GET](https://redis.io/docs/latest/commands/slowlog-get/)

## 9. Run a Lua script

**Purpose:** Evaluate a script when a single command cannot express the operation. Replace the script, the number of keys, and the keys and arguments.

```text
EVAL "return redis.call('GET', KEYS[1])" 1 key
```

**Result:** Whatever the script returns, mapped by the same rules as a command reply. `EVAL` is an unknown operation for the approval gate: it can read, write or delete anything, so treat it like an unrestricted statement and never replay it after an uncertain outcome.

**Official documentation:** [EVAL](https://redis.io/docs/latest/commands/eval/)

A reply becomes a table by these rules: a scalar fills one `value` row; a flat array fills one `value` row per element; `HGETALL` and `CONFIG GET` fill `field`/`value`; an array of arrays fills `c1..cN`; any other nested reply fills one row whose nested cells hold JSON text. Integers come back as the `integer` type, and a string that is not valid UTF-8 comes back base64-encoded with the column marked `base64`. Redis has no affected-row count, so `affected_rows` is null and the count a write command returns appears as its own row. Write commands are the ones that change keys, expiries or server state, including `SET`, `DEL`, `EXPIRE`, `FLUSHDB`, `FLUSHALL` and `EVAL`; confirm the target and the scope with the user before running them, and remember that Redis applies each command immediately with no rollback.
