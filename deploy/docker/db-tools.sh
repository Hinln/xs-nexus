#!/usr/bin/env bash
set -Eeuo pipefail

DATABASE_URL_FILE=${DATABASE_URL_FILE:-/run/secrets/xs-controller/database-url}
BACKUP_DIRECTORY=${BACKUP_DIRECTORY:-/backups}
DATABASE_SCHEMA=${DATABASE_SCHEMA:-}
DATABASE_CONNECTION_URI=

fail() {
    printf 'xs-db-tools: %s\n' "$1" >&2
    exit 1
}

validate_schema() {
    [[ $DATABASE_SCHEMA =~ ^[a-z_][a-z0-9_]{0,62}$ ]] || fail 'invalid DATABASE_SCHEMA'
}

validate_backup_name() {
    [[ ${1:-} =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,95}$ ]] || fail 'invalid BACKUP_NAME'
}

load_database_url() {
    [[ ! -L $DATABASE_URL_FILE && -f $DATABASE_URL_FILE ]] || fail 'database URL secret must be a regular non-symlink file'
    local mode size
    mode=$(stat -c '%a' "$DATABASE_URL_FILE")
    size=$(stat -c '%s' "$DATABASE_URL_FILE")
    (( (8#$mode & 077) == 0 )) || fail 'database URL secret permissions are too broad'
    (( size > 0 && size <= 8192 )) || fail 'database URL secret length is invalid'
    DATABASE_CONNECTION_URI=$(cat -- "$DATABASE_URL_FILE")
    [[ -n $DATABASE_CONNECTION_URI && $DATABASE_CONNECTION_URI != *$'\n'* && $DATABASE_CONNECTION_URI != *$'\r'* ]] || fail 'database URL secret value is invalid'
}

schema_exists() {
    local result
    result=$(psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --tuples-only --no-align --set ON_ERROR_STOP=1 \
        --command "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = '$DATABASE_SCHEMA')")
    [[ $result == t ]]
}

backup_paths() {
    local name=$1
    ARCHIVE_PATH="$BACKUP_DIRECTORY/$name.dump"
    MANIFEST_PATH="$BACKUP_DIRECTORY/$name.manifest"
}

create_backup() {
    local name=$1 temporary archive_size archive_hash created_at
    validate_backup_name "$name"
    backup_paths "$name"
    [[ ! -e $ARCHIVE_PATH && ! -e $MANIFEST_PATH ]] || fail 'backup already exists'
    temporary=$(mktemp -d "$BACKUP_DIRECTORY/.xs-backup.XXXXXX")
    trap 'rm -rf -- "$temporary"' RETURN
    pg_dump --dbname="$DATABASE_CONNECTION_URI" --format=custom --no-owner --no-acl --schema="$DATABASE_SCHEMA" \
        --file="$temporary/archive.dump"
    pg_restore --list "$temporary/archive.dump" >/dev/null
    archive_size=$(stat -c '%s' "$temporary/archive.dump")
    archive_hash=$(sha256sum "$temporary/archive.dump" | awk '{print $1}')
    created_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    {
        printf 'schema=%s\n' "$DATABASE_SCHEMA"
        printf 'archive=%s.dump\n' "$name"
        printf 'bytes=%s\n' "$archive_size"
        printf 'sha256=%s\n' "$archive_hash"
        printf 'created_at=%s\n' "$created_at"
    } >"$temporary/manifest"
    chmod 0600 "$temporary/archive.dump" "$temporary/manifest"
    mv -- "$temporary/archive.dump" "$ARCHIVE_PATH"
    mv -- "$temporary/manifest" "$MANIFEST_PATH"
    trap - RETURN
    rm -rf -- "$temporary"
    printf 'backup=%s\n' "$name"
}

verify_backup() {
    local name=$1 archive_size archive_hash
    local -a lines
    validate_backup_name "$name"
    backup_paths "$name"
    [[ ! -L $ARCHIVE_PATH && -f $ARCHIVE_PATH ]] || fail 'backup archive is missing or invalid'
    [[ ! -L $MANIFEST_PATH && -f $MANIFEST_PATH ]] || fail 'backup manifest is missing or invalid'
    mapfile -t lines <"$MANIFEST_PATH"
    [[ ${#lines[@]} -eq 5 ]] || fail 'backup manifest field count is invalid'
    [[ ${lines[0]} == "schema=$DATABASE_SCHEMA" ]] || fail 'backup schema does not match target'
    [[ ${lines[1]} == "archive=$name.dump" ]] || fail 'backup archive name is invalid'
    [[ ${lines[2]} =~ ^bytes=([0-9]+)$ ]] || fail 'backup size field is invalid'
    archive_size=${BASH_REMATCH[1]}
    [[ ${lines[3]} =~ ^sha256=([0-9a-f]{64})$ ]] || fail 'backup hash field is invalid'
    archive_hash=${BASH_REMATCH[1]}
    [[ ${lines[4]} =~ ^created_at=[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] || fail 'backup timestamp is invalid'
    [[ $(stat -c '%s' "$ARCHIVE_PATH") == "$archive_size" ]] || fail 'backup archive size mismatch'
    [[ $(sha256sum "$ARCHIVE_PATH" | awk '{print $1}') == "$archive_hash" ]] || fail 'backup archive hash mismatch'
    pg_restore --list "$ARCHIVE_PATH" >/dev/null
}

drop_schema() {
    psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --set ON_ERROR_STOP=1 \
        --command "DROP SCHEMA IF EXISTS \"$DATABASE_SCHEMA\" CASCADE" >/dev/null
}

restore_archive() {
    local archive=$1
    psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --set ON_ERROR_STOP=1 \
        --command "CREATE SCHEMA \"$DATABASE_SCHEMA\"" >/dev/null
    pg_restore --exit-on-error --no-owner --no-acl --schema="$DATABASE_SCHEMA" \
        --dbname="$DATABASE_CONNECTION_URI" "$archive"
}

restore_backup() {
    local name=$1 confirmation=${CONFIRM_SCHEMA:-} safety_name='' restore_status
    [[ $confirmation == "$DATABASE_SCHEMA" ]] || fail 'CONFIRM_SCHEMA must exactly match DATABASE_SCHEMA'
    verify_backup "$name"
    if schema_exists; then
        safety_name="pre-restore-$DATABASE_SCHEMA-$(date -u +%Y%m%dT%H%M%SZ)"
        create_backup "$safety_name" >/dev/null
    fi
    backup_paths "$name"
    drop_schema
    set +e
    restore_archive "$ARCHIVE_PATH"
    restore_status=$?
    set -e
    if (( restore_status != 0 )); then
        drop_schema
        if [[ -n $safety_name ]]; then
            verify_backup "$safety_name"
            restore_archive "$ARCHIVE_PATH" || fail 'restore failed and safety rollback also failed'
        fi
        fail 'restore failed; previous schema state was restored'
    fi
    printf 'restored=%s\n' "$name"
    [[ -z $safety_name ]] || printf 'safety_backup=%s\n' "$safety_name"
}

main() {
    validate_schema
    [[ -d $BACKUP_DIRECTORY && ! -L $BACKUP_DIRECTORY ]] || fail 'backup directory is missing or invalid'
    load_database_url
    umask 077
    case ${1:-} in
        schema-exists)
            schema_exists || exit 3
            ;;
        backup)
            schema_exists || fail 'database schema does not exist'
            create_backup "${BACKUP_NAME:-}"
            ;;
        verify)
            verify_backup "${BACKUP_NAME:-}"
            printf 'verified=%s\n' "$BACKUP_NAME"
            ;;
        restore)
            restore_backup "${BACKUP_NAME:-}"
            ;;
        *)
            fail 'usage: xs-db-tools schema-exists|backup|verify|restore'
            ;;
    esac
}

main "$@"
