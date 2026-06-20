#!/bin/sh
set -eu

database_url="${NETWATCH_DATABASE_URL:-sqlite://netwatch.db}"

case "$database_url" in
    sqlite://*)
        db_path="${database_url#sqlite://}"
        ;;
    *)
        echo "只支持 sqlite:// 数据库地址：$database_url" >&2
        exit 1
        ;;
esac

if [ -z "$db_path" ]; then
    echo "数据库路径为空" >&2
    exit 1
fi

rm -f "$db_path" "$db_path-shm" "$db_path-wal"

NETWATCH_DATABASE_URL="$database_url" cargo run -- --migrate-only
