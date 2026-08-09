#!/usr/bin/env bash
set -Eeuo pipefail

DATABASE_URL_FILE=${DATABASE_URL_FILE:-/run/secrets/xs-controller/database-url}
BACKUP_RECIPIENT_FILE=${BACKUP_RECIPIENT_FILE:-/run/secrets/xs-controller/backup-recipient}
BACKUP_IDENTITY_FILE=${BACKUP_IDENTITY_FILE:-}
BACKUP_DIRECTORY=${BACKUP_DIRECTORY:-/backups}
REPLICA_DIRECTORY=${REPLICA_DIRECTORY:-/replica}
DATABASE_SCHEMA=${DATABASE_SCHEMA:-}
DATABASE_OWNER_ROLE=${DATABASE_OWNER_ROLE:-}
DEPLOYMENT_NAME=${DEPLOYMENT_NAME:-}
SKIP_SAFETY_BACKUP=${SKIP_SAFETY_BACKUP:-false}
LOCAL_RETENTION_DAYS=${LOCAL_RETENTION_DAYS:-30}
REPLICA_RETENTION_DAYS=${REPLICA_RETENTION_DAYS:-180}
MIN_RETAINED_BACKUPS=${MIN_RETAINED_BACKUPS:-3}
DATABASE_CONNECTION_URI=
RECIPIENT_KEY_ID=
REPLICA_TARGET_ID=

fail() {
    printf 'xs-db-tools: %s\n' "$1" >&2
    exit 1
}

validate_schema() {
    [[ $DATABASE_SCHEMA =~ ^[a-z_][a-z0-9_]{0,62}$ ]] || fail 'invalid DATABASE_SCHEMA'
}

validate_database_role() {
    [[ $DATABASE_OWNER_ROLE =~ ^[a-z_][a-z0-9_]{0,62}$ ]] || fail 'invalid DATABASE_OWNER_ROLE'
}

validate_backup_name() {
    [[ ${1:-} =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,95}$ ]] || fail 'invalid BACKUP_NAME'
}

validate_private_directory() {
    local path=$1 mode owner
    [[ $path == /* && ! -L $path && -d $path ]] || fail "private directory is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    [[ $owner == "$(id -u)" ]] || fail "private directory owner is invalid: $path"
    (( (8#$mode & 077) == 0 )) || fail "private directory permissions are too broad: $path"
}

validate_private_file() {
    local path=$1 maximum_size=$2 mode owner size
    [[ ! -L $path && -f $path ]] || fail "private file is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    size=$(stat -c '%s' "$path")
    [[ $owner == "$(id -u)" ]] || fail "private file owner is invalid: $path"
    (( (8#$mode & 077) == 0 )) || fail "private file permissions are too broad: $path"
    (( size > 0 && size <= maximum_size )) || fail "private file length is invalid: $path"
}

validate_stored_file() {
    local path=$1 mode owner
    [[ ! -L $path && -f $path ]] || fail "backup file is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    [[ $owner == "$(id -u)" ]] || fail "backup file owner is invalid: $path"
    (( (8#$mode & 077) == 0 )) || fail "backup file permissions are too broad: $path"
}

bounded_integer() {
    local name=$1 value=$2 minimum=$3 maximum=$4
    [[ $value =~ ^[0-9]+$ ]] || fail "$name is invalid"
    (( value >= minimum && value <= maximum )) || fail "$name is out of range"
}

load_database_url() {
    validate_private_file "$DATABASE_URL_FILE" 8192
    DATABASE_CONNECTION_URI=$(<"$DATABASE_URL_FILE")
    [[ -n $DATABASE_CONNECTION_URI && $DATABASE_CONNECTION_URI != *$'\n'* && $DATABASE_CONNECTION_URI != *$'\r'* ]] || fail 'database URL secret value is invalid'
}

load_recipient() {
    local -a lines
    validate_private_file "$BACKUP_RECIPIENT_FILE" 256
    mapfile -t lines <"$BACKUP_RECIPIENT_FILE"
    [[ ${#lines[@]} -eq 1 && ${lines[0]} =~ ^age1[0-9a-z]{58}$ ]] || fail 'backup recipient is invalid'
    RECIPIENT_KEY_ID=$(sha256sum "$BACKUP_RECIPIENT_FILE" | awk '{print $1}')
}

load_identity() {
    [[ -n $BACKUP_IDENTITY_FILE ]] || fail 'backup identity is required for this operation'
    validate_private_file "$BACKUP_IDENTITY_FILE" 65536
}

load_replica_target() {
    local marker="$REPLICA_DIRECTORY/.xs-nexus-replica"
    local -a lines
    validate_stored_file "$marker"
    mapfile -t lines <"$marker"
    [[ ${#lines[@]} -eq 3 ]] || fail 'replica marker field count is invalid'
    [[ ${lines[0]} == 'format=xs-nexus-replica-v1' ]] || fail 'replica marker format is invalid'
    [[ ${lines[1]} == "deployment=$DEPLOYMENT_NAME" ]] || fail 'replica marker deployment is invalid'
    [[ ${lines[2]} =~ ^target_id=([A-Za-z0-9][A-Za-z0-9._:-]{0,127})$ ]] || fail 'replica target ID is invalid'
    REPLICA_TARGET_ID=${BASH_REMATCH[1]}
}

validate_storage() {
    validate_private_directory "$BACKUP_DIRECTORY"
    validate_private_directory "$REPLICA_DIRECTORY"
    [[ $(stat -c '%d' "$BACKUP_DIRECTORY") != "$(stat -c '%d' "$REPLICA_DIRECTORY")" ]] || fail 'replica directory must be a distinct mounted filesystem'
    [[ $DEPLOYMENT_NAME =~ ^(dev|rc)$ ]] || fail 'invalid DEPLOYMENT_NAME'
    bounded_integer LOCAL_RETENTION_DAYS "$LOCAL_RETENTION_DAYS" 0 3650
    bounded_integer REPLICA_RETENTION_DAYS "$REPLICA_RETENTION_DAYS" 0 3650
    bounded_integer MIN_RETAINED_BACKUPS "$MIN_RETAINED_BACKUPS" 1 1000
    (( REPLICA_RETENTION_DAYS >= LOCAL_RETENTION_DAYS )) || fail 'replica retention must not be shorter than local retention'
    load_replica_target
}

schema_exists() {
    local result
    result=$(psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --tuples-only --no-align --set ON_ERROR_STOP=1 \
        --command "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = '$DATABASE_SCHEMA')")
    [[ $result == t ]]
}

set_paths() {
    local directory=$1 name=$2 receipt_suffix=$3
    ARCHIVE_PATH="$directory/$name.dump.age"
    MANIFEST_PATH="$directory/$name.manifest.age"
    INDEX_PATH="$directory/$name.index"
    RECEIPT_PATH="$directory/$name.$receipt_suffix"
}

read_index() {
    local index=$1 expected_name=$2
    local -a lines
    validate_stored_file "$index"
    mapfile -t lines <"$index"
    [[ ${#lines[@]} -eq 11 ]] || fail 'backup index field count is invalid'
    [[ ${lines[0]} == 'format=xs-nexus-encrypted-backup-v1' ]] || fail 'backup index format is invalid'
    [[ ${lines[1]} == "schema=$DATABASE_SCHEMA" ]] || fail 'backup schema does not match target'
    [[ ${lines[2]} == "backup=$expected_name" ]] || fail 'backup name is invalid'
    [[ ${lines[3]} == "archive=$expected_name.dump.age" ]] || fail 'backup archive name is invalid'
    [[ ${lines[4]} =~ ^archive_bytes=([0-9]+)$ ]] || fail 'backup archive size field is invalid'
    INDEX_ARCHIVE_BYTES=${BASH_REMATCH[1]}
    [[ ${lines[5]} =~ ^archive_sha256=([0-9a-f]{64})$ ]] || fail 'backup archive hash field is invalid'
    INDEX_ARCHIVE_HASH=${BASH_REMATCH[1]}
    [[ ${lines[6]} == "manifest=$expected_name.manifest.age" ]] || fail 'backup manifest name is invalid'
    [[ ${lines[7]} =~ ^manifest_bytes=([0-9]+)$ ]] || fail 'backup manifest size field is invalid'
    INDEX_MANIFEST_BYTES=${BASH_REMATCH[1]}
    [[ ${lines[8]} =~ ^manifest_sha256=([0-9a-f]{64})$ ]] || fail 'backup manifest hash field is invalid'
    INDEX_MANIFEST_HASH=${BASH_REMATCH[1]}
    [[ ${lines[9]} =~ ^recipient_key_id=([0-9a-f]{64})$ ]] || fail 'backup recipient key ID is invalid'
    INDEX_RECIPIENT_KEY_ID=${BASH_REMATCH[1]}
    [[ ${lines[10]} =~ ^created_at=([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z)$ ]] || fail 'backup timestamp is invalid'
    INDEX_CREATED_AT=${BASH_REMATCH[1]}
}

verify_bundle() {
    local directory=$1 name=$2 receipt_suffix=$3
    validate_backup_name "$name"
    set_paths "$directory" "$name" "$receipt_suffix"
    read_index "$INDEX_PATH" "$name"
    validate_stored_file "$ARCHIVE_PATH"
    validate_stored_file "$MANIFEST_PATH"
    [[ $(stat -c '%s' "$ARCHIVE_PATH") == "$INDEX_ARCHIVE_BYTES" ]] || fail 'encrypted archive size mismatch'
    [[ $(sha256sum "$ARCHIVE_PATH" | awk '{print $1}') == "$INDEX_ARCHIVE_HASH" ]] || fail 'encrypted archive hash mismatch'
    [[ $(stat -c '%s' "$MANIFEST_PATH") == "$INDEX_MANIFEST_BYTES" ]] || fail 'encrypted manifest size mismatch'
    [[ $(sha256sum "$MANIFEST_PATH" | awk '{print $1}') == "$INDEX_MANIFEST_HASH" ]] || fail 'encrypted manifest hash mismatch'
}

write_replication_receipt() {
    local destination=$1 name=$2 index_hash=$3 archive_hash=$4 manifest_hash=$5 replicated_at=$6 temporary
    temporary=$(mktemp "$destination/.xs-receipt.XXXXXX")
    {
        printf 'format=xs-nexus-replication-v1\n'
        printf 'backup=%s\n' "$name"
        printf 'target_id=%s\n' "$REPLICA_TARGET_ID"
        printf 'archive_sha256=%s\n' "$archive_hash"
        printf 'manifest_sha256=%s\n' "$manifest_hash"
        printf 'index_sha256=%s\n' "$index_hash"
        printf 'replicated_at=%s\n' "$replicated_at"
    } >"$temporary"
    chmod 0600 "$temporary"
    printf '%s\n' "$temporary"
}

validate_replication_receipt() {
    local path=$1 name=$2 archive_hash=$3 manifest_hash=$4 index_hash=$5
    local -a lines
    validate_stored_file "$path"
    mapfile -t lines <"$path"
    [[ ${#lines[@]} -eq 7 ]] || fail 'replication receipt field count is invalid'
    [[ ${lines[0]} == 'format=xs-nexus-replication-v1' ]] || fail 'replication receipt format is invalid'
    [[ ${lines[1]} == "backup=$name" ]] || fail 'replication receipt backup is invalid'
    [[ ${lines[2]} == "target_id=$REPLICA_TARGET_ID" ]] || fail 'replication receipt target is invalid'
    [[ ${lines[3]} == "archive_sha256=$archive_hash" ]] || fail 'replication receipt archive hash is invalid'
    [[ ${lines[4]} == "manifest_sha256=$manifest_hash" ]] || fail 'replication receipt manifest hash is invalid'
    [[ ${lines[5]} == "index_sha256=$index_hash" ]] || fail 'replication receipt index hash is invalid'
    [[ ${lines[6]} =~ ^replicated_at=[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] || fail 'replication receipt timestamp is invalid'
}

replicate_backup() {
    local name=$1 index_hash archive_hash manifest_hash replicated_at temporary replica_receipt local_receipt
    verify_bundle "$BACKUP_DIRECTORY" "$name" replicated
    index_hash=$(sha256sum "$INDEX_PATH" | awk '{print $1}')
    archive_hash=$INDEX_ARCHIVE_HASH
    manifest_hash=$INDEX_MANIFEST_HASH
    temporary=$(mktemp -d "$REPLICA_DIRECTORY/.xs-replica.XXXXXX")
    trap 'rm -rf -- "$temporary"' RETURN
    cp -- "$ARCHIVE_PATH" "$temporary/$name.dump.age"
    cp -- "$MANIFEST_PATH" "$temporary/$name.manifest.age"
    cp -- "$INDEX_PATH" "$temporary/$name.index"
    chmod 0600 "$temporary"/*
    for source in "$temporary/$name.dump.age" "$temporary/$name.manifest.age" "$temporary/$name.index"; do
        local destination="$REPLICA_DIRECTORY/${source##*/}"
        if [[ -e $destination ]]; then
            validate_stored_file "$destination"
            [[ $(sha256sum "$source" | awk '{print $1}') == "$(sha256sum "$destination" | awk '{print $1}')" ]] || fail 'replica contains a conflicting immutable backup'
        else
            mv -- "$source" "$destination"
        fi
    done
    set_paths "$REPLICA_DIRECTORY" "$name" replication
    if [[ -e $RECEIPT_PATH ]]; then
        validate_replication_receipt "$RECEIPT_PATH" "$name" "$archive_hash" "$manifest_hash" "$index_hash"
    else
        replicated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        replica_receipt=$(write_replication_receipt "$REPLICA_DIRECTORY" "$name" "$index_hash" "$archive_hash" "$manifest_hash" "$replicated_at")
        mv -- "$replica_receipt" "$RECEIPT_PATH"
    fi
    replica_receipt=$RECEIPT_PATH
    set_paths "$BACKUP_DIRECTORY" "$name" replicated
    if [[ -e $RECEIPT_PATH ]]; then
        cmp -s "$replica_receipt" "$RECEIPT_PATH" || fail 'local replication receipt conflicts with existing state'
    else
        local_receipt=$(mktemp "$BACKUP_DIRECTORY/.xs-receipt.XXXXXX")
        cp -- "$replica_receipt" "$local_receipt"
        chmod 0600 "$local_receipt"
        mv -- "$local_receipt" "$RECEIPT_PATH"
    fi
    trap - RETURN
    rm -rf -- "$temporary"
}

verify_backup() {
    local name=$1 local_archive_hash local_manifest_hash local_index_hash
    verify_bundle "$BACKUP_DIRECTORY" "$name" replicated
    local_archive_hash=$INDEX_ARCHIVE_HASH
    local_manifest_hash=$INDEX_MANIFEST_HASH
    local_index_hash=$(sha256sum "$INDEX_PATH" | awk '{print $1}')
    local local_receipt=$RECEIPT_PATH
    verify_bundle "$REPLICA_DIRECTORY" "$name" replication
    [[ $INDEX_ARCHIVE_HASH == "$local_archive_hash" && $INDEX_MANIFEST_HASH == "$local_manifest_hash" ]] || fail 'replica backup hashes do not match local backup'
    [[ $(sha256sum "$INDEX_PATH" | awk '{print $1}') == "$local_index_hash" ]] || fail 'replica index does not match local backup'
    local replica_receipt=$RECEIPT_PATH
    validate_replication_receipt "$local_receipt" "$name" "$local_archive_hash" "$local_manifest_hash" "$local_index_hash"
    validate_replication_receipt "$replica_receipt" "$name" "$local_archive_hash" "$local_manifest_hash" "$local_index_hash"
    cmp -s "$local_receipt" "$replica_receipt" || fail 'replication receipts do not match'
}

consume_custom_archive() {
    local header
    LC_ALL=C IFS= read -r -N 5 header || return 1
    [[ $header == PGDMP ]] || return 1
    cat >/dev/null
}

create_backup() {
    local name=$1 encrypted_temporary plaintext_temporary archive_size archive_hash
    local manifest_size manifest_hash created_at validator_pid validator_status
    local -a pipeline_status
    validate_backup_name "$name"
    set_paths "$BACKUP_DIRECTORY" "$name" replicated
    [[ ! -e $ARCHIVE_PATH && ! -e $MANIFEST_PATH && ! -e $INDEX_PATH && ! -e $RECEIPT_PATH ]] || fail 'backup already exists'
    set_paths "$REPLICA_DIRECTORY" "$name" replication
    [[ ! -e $ARCHIVE_PATH && ! -e $MANIFEST_PATH && ! -e $INDEX_PATH && ! -e $RECEIPT_PATH ]] || fail 'replica backup already exists'
    [[ ! -e $REPLICA_DIRECTORY/.destroyed/$name.tombstone ]] || fail 'backup name was previously destroyed and cannot be reused'
    encrypted_temporary=$(mktemp -d "$BACKUP_DIRECTORY/.xs-backup.XXXXXX")
    plaintext_temporary=$(mktemp -d /tmp/xs-backup.XXXXXX)
    trap 'rm -rf -- "$encrypted_temporary" "$plaintext_temporary"' RETURN
    mkfifo "$plaintext_temporary/archive.pipe"
    consume_custom_archive <"$plaintext_temporary/archive.pipe" &
    validator_pid=$!
    set +e
    pg_dump --dbname="$DATABASE_CONNECTION_URI" --role="$DATABASE_OWNER_ROLE" \
        --format=custom --no-owner --no-acl --schema="$DATABASE_SCHEMA" | \
        tee "$plaintext_temporary/archive.pipe" | \
        age --encrypt -R "$BACKUP_RECIPIENT_FILE" --output "$encrypted_temporary/$name.dump.age"
    pipeline_status=("${PIPESTATUS[@]}")
    wait "$validator_pid"
    validator_status=$?
    set -e
    if [[ ${pipeline_status[*]} != '0 0 0' || $validator_status -ne 0 ]]; then
        printf 'xs-db-tools: backup pipeline status pg_dump=%s tee=%s age=%s archive_validator=%s\n' \
            "${pipeline_status[0]:-missing}" "${pipeline_status[1]:-missing}" \
            "${pipeline_status[2]:-missing}" "$validator_status" >&2
        fail 'streaming encrypted backup validation failed'
    fi
    archive_size=$(stat -c '%s' "$encrypted_temporary/$name.dump.age")
    archive_hash=$(sha256sum "$encrypted_temporary/$name.dump.age" | awk '{print $1}')
    created_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    {
        printf 'format=xs-nexus-authenticated-backup-v1\n'
        printf 'schema=%s\n' "$DATABASE_SCHEMA"
        printf 'backup=%s\n' "$name"
        printf 'archive=%s.dump.age\n' "$name"
        printf 'archive_bytes=%s\n' "$archive_size"
        printf 'archive_sha256=%s\n' "$archive_hash"
        printf 'recipient_key_id=%s\n' "$RECIPIENT_KEY_ID"
        printf 'created_at=%s\n' "$created_at"
    } >"$plaintext_temporary/authenticated.manifest"
    age --encrypt -R "$BACKUP_RECIPIENT_FILE" \
        --output "$encrypted_temporary/$name.manifest.age" \
        "$plaintext_temporary/authenticated.manifest"
    manifest_size=$(stat -c '%s' "$encrypted_temporary/$name.manifest.age")
    manifest_hash=$(sha256sum "$encrypted_temporary/$name.manifest.age" | awk '{print $1}')
    {
        printf 'format=xs-nexus-encrypted-backup-v1\n'
        printf 'schema=%s\n' "$DATABASE_SCHEMA"
        printf 'backup=%s\n' "$name"
        printf 'archive=%s.dump.age\n' "$name"
        printf 'archive_bytes=%s\n' "$archive_size"
        printf 'archive_sha256=%s\n' "$archive_hash"
        printf 'manifest=%s.manifest.age\n' "$name"
        printf 'manifest_bytes=%s\n' "$manifest_size"
        printf 'manifest_sha256=%s\n' "$manifest_hash"
        printf 'recipient_key_id=%s\n' "$RECIPIENT_KEY_ID"
        printf 'created_at=%s\n' "$created_at"
    } >"$encrypted_temporary/$name.index"
    chmod 0600 "$encrypted_temporary"/*
    mv -- "$encrypted_temporary/$name.dump.age" "$BACKUP_DIRECTORY/$name.dump.age"
    mv -- "$encrypted_temporary/$name.manifest.age" "$BACKUP_DIRECTORY/$name.manifest.age"
    mv -- "$encrypted_temporary/$name.index" "$BACKUP_DIRECTORY/$name.index"
    trap - RETURN
    rm -rf -- "$encrypted_temporary" "$plaintext_temporary"
    replicate_backup "$name"
    printf 'backup=%s\n' "$name"
    printf 'encryption=age-x25519\n'
    printf 'replica_target=%s\n' "$REPLICA_TARGET_ID"
}

authenticate_backup() {
    local directory=$1 name=$2 receipt_suffix=$3 temporary index_hash
    local -a lines pipeline_status
    verify_bundle "$directory" "$name" "$receipt_suffix"
    local expected_archive_bytes=$INDEX_ARCHIVE_BYTES
    local expected_archive_hash=$INDEX_ARCHIVE_HASH
    local expected_manifest_hash=$INDEX_MANIFEST_HASH
    local expected_recipient_key_id=$INDEX_RECIPIENT_KEY_ID
    local expected_created_at=$INDEX_CREATED_AT
    index_hash=$(sha256sum "$INDEX_PATH" | awk '{print $1}')
    validate_replication_receipt "$RECEIPT_PATH" "$name" "$expected_archive_hash" \
        "$expected_manifest_hash" "$index_hash"
    temporary=$(mktemp -d /tmp/xs-authenticated-backup.XXXXXX)
    trap 'rm -rf -- "$temporary"' RETURN
    age --decrypt -i "$BACKUP_IDENTITY_FILE" --output "$temporary/manifest" "$MANIFEST_PATH" || fail 'unable to decrypt authenticated backup manifest'
    mapfile -t lines <"$temporary/manifest"
    [[ ${#lines[@]} -eq 8 ]] || fail 'authenticated backup manifest field count is invalid'
    [[ ${lines[0]} == 'format=xs-nexus-authenticated-backup-v1' ]] || fail 'authenticated backup manifest format is invalid'
    [[ ${lines[1]} == "schema=$DATABASE_SCHEMA" ]] || fail 'authenticated backup schema is invalid'
    [[ ${lines[2]} == "backup=$name" ]] || fail 'authenticated backup name is invalid'
    [[ ${lines[3]} == "archive=$name.dump.age" ]] || fail 'authenticated backup archive is invalid'
    [[ ${lines[4]} == "archive_bytes=$expected_archive_bytes" ]] || fail 'authenticated backup size is invalid'
    [[ ${lines[5]} == "archive_sha256=$expected_archive_hash" ]] || fail 'authenticated backup hash is invalid'
    [[ ${lines[6]} == "recipient_key_id=$expected_recipient_key_id" ]] || fail 'authenticated backup recipient is invalid'
    [[ ${lines[7]} == "created_at=$expected_created_at" ]] || fail 'authenticated backup timestamp is invalid'
    age --decrypt -i "$BACKUP_IDENTITY_FILE" "$ARCHIVE_PATH" >/dev/null || fail 'encrypted backup archive authentication failed'
    set +e
    age --decrypt -i "$BACKUP_IDENTITY_FILE" "$ARCHIVE_PATH" | pg_restore --list >/dev/null
    pipeline_status=("${PIPESTATUS[@]}")
    set -e
    [[ ${pipeline_status[1]:-missing} == 0 ]] || fail 'decrypted backup archive validation failed'
    [[ ${pipeline_status[0]:-missing} == 0 || ${pipeline_status[0]:-missing} == 141 ]] || fail 'decrypted backup archive stream failed'
    trap - RETURN
    rm -rf -- "$temporary"
}

fetch_backup() {
    local name=$1 temporary
    validate_backup_name "$name"
    set_paths "$BACKUP_DIRECTORY" "$name" replicated
    [[ ! -e $ARCHIVE_PATH && ! -e $MANIFEST_PATH && ! -e $INDEX_PATH && ! -e $RECEIPT_PATH ]] || fail 'local backup already exists'
    verify_bundle "$REPLICA_DIRECTORY" "$name" replication
    local replica_receipt=$RECEIPT_PATH
    local archive_hash=$INDEX_ARCHIVE_HASH manifest_hash=$INDEX_MANIFEST_HASH
    local index_hash
    index_hash=$(sha256sum "$INDEX_PATH" | awk '{print $1}')
    validate_replication_receipt "$replica_receipt" "$name" "$archive_hash" "$manifest_hash" "$index_hash"
    temporary=$(mktemp -d "$BACKUP_DIRECTORY/.xs-fetch.XXXXXX")
    trap 'rm -rf -- "$temporary"' RETURN
    cp -- "$ARCHIVE_PATH" "$temporary/$name.dump.age"
    cp -- "$MANIFEST_PATH" "$temporary/$name.manifest.age"
    cp -- "$INDEX_PATH" "$temporary/$name.index"
    cp -- "$replica_receipt" "$temporary/$name.replicated"
    chmod 0600 "$temporary"/*
    mv -- "$temporary/$name.dump.age" "$BACKUP_DIRECTORY/$name.dump.age"
    mv -- "$temporary/$name.manifest.age" "$BACKUP_DIRECTORY/$name.manifest.age"
    mv -- "$temporary/$name.index" "$BACKUP_DIRECTORY/$name.index"
    mv -- "$temporary/$name.replicated" "$BACKUP_DIRECTORY/$name.replicated"
    trap - RETURN
    rm -rf -- "$temporary"
    verify_backup "$name"
    printf 'fetched=%s\n' "$name"
}

drop_schema() {
    psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --set ON_ERROR_STOP=1 \
        --command "SET ROLE \"$DATABASE_OWNER_ROLE\"; DROP SCHEMA IF EXISTS \"$DATABASE_SCHEMA\" CASCADE" >/dev/null
}

restore_archive() {
    local archive=$1
    local -a pipeline_status
    psql --dbname="$DATABASE_CONNECTION_URI" --no-psqlrc --set ON_ERROR_STOP=1 \
        --command "SET ROLE \"$DATABASE_OWNER_ROLE\"; CREATE SCHEMA \"$DATABASE_SCHEMA\" AUTHORIZATION \"$DATABASE_OWNER_ROLE\"" >/dev/null
    set +e
    age --decrypt -i "$BACKUP_IDENTITY_FILE" "$archive" | \
        pg_restore --exit-on-error --no-owner --no-acl --role="$DATABASE_OWNER_ROLE" --schema="$DATABASE_SCHEMA" \
            --dbname="$DATABASE_CONNECTION_URI"
    pipeline_status=("${PIPESTATUS[@]}")
    set -e
    [[ ${pipeline_status[*]} == '0 0' ]]
}

restore_backup() {
    local name=$1 confirmation=${CONFIRM_SCHEMA:-} safety_name='' restore_status
    [[ $confirmation == "$DATABASE_SCHEMA" ]] || fail 'CONFIRM_SCHEMA must exactly match DATABASE_SCHEMA'
    verify_backup "$name"
    authenticate_backup "$BACKUP_DIRECTORY" "$name" replicated
    if schema_exists && [[ $SKIP_SAFETY_BACKUP == false ]]; then
        load_recipient
        safety_name="pre-restore-$DATABASE_SCHEMA-$(date -u +%Y%m%dT%H%M%SZ)"
        create_backup "$safety_name" >/dev/null
        authenticate_backup "$BACKUP_DIRECTORY" "$safety_name" replicated
    fi
    set_paths "$BACKUP_DIRECTORY" "$name" replicated
    local restore_path=$ARCHIVE_PATH
    drop_schema
    set +e
    restore_archive "$restore_path"
    restore_status=$?
    set -e
    if (( restore_status != 0 )); then
        drop_schema
        if [[ -n $safety_name ]]; then
            authenticate_backup "$BACKUP_DIRECTORY" "$safety_name" replicated
            set_paths "$BACKUP_DIRECTORY" "$safety_name" replicated
            restore_archive "$ARCHIVE_PATH" || fail 'restore failed and safety rollback also failed'
        fi
        fail 'restore failed; previous schema state was restored'
    fi
    printf 'restored=%s\n' "$name"
    [[ -z $safety_name ]] || printf 'safety_backup=%s\n' "$safety_name"
}

write_tombstone() {
    local name=$1 archive_hash=$2 manifest_hash=$3 index_hash=$4 created_at=$5 temporary directory
    directory="$REPLICA_DIRECTORY/.destroyed"
    [[ -e $directory ]] || mkdir -m 0700 "$directory"
    validate_private_directory "$directory"
    [[ ! -e $directory/$name.tombstone ]] || fail 'backup destruction tombstone already exists'
    temporary=$(mktemp "$directory/.xs-tombstone.XXXXXX")
    {
        printf 'format=xs-nexus-backup-destruction-v1\n'
        printf 'backup=%s\n' "$name"
        printf 'target_id=%s\n' "$REPLICA_TARGET_ID"
        printf 'archive_sha256=%s\n' "$archive_hash"
        printf 'manifest_sha256=%s\n' "$manifest_hash"
        printf 'index_sha256=%s\n' "$index_hash"
        printf 'created_at=%s\n' "$created_at"
        printf 'destroyed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'policy=authenticated-retention-v1\n'
    } >"$temporary"
    chmod 0600 "$temporary"
    mv -- "$temporary" "$directory/$name.tombstone"
}

remove_local_bundle() {
    local name=$1 removed=0
    set_paths "$BACKUP_DIRECTORY" "$name" replicated
    for path in "$ARCHIVE_PATH" "$MANIFEST_PATH" "$INDEX_PATH" "$RECEIPT_PATH"; do
        if [[ -e $path ]]; then
            rm -- "$path"
            removed=1
        fi
    done
    (( removed == 1 ))
}

remove_replica_bundle() {
    local name=$1
    set_paths "$REPLICA_DIRECTORY" "$name" replication
    for path in "$ARCHIVE_PATH" "$MANIFEST_PATH" "$INDEX_PATH" "$RECEIPT_PATH"; do
        [[ ! -e $path ]] || rm -- "$path"
    done
}

prune_backups() {
    local confirmation=${CONFIRM_BEFORE:-} confirm_epoch now_epoch local_cutoff replica_cutoff
    local path name created_epoch index_hash archive_hash manifest_hash retained=0
    local local_pruned=0 replica_destroyed=0
    local -a records=()
    [[ $confirmation =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] || fail 'CONFIRM_BEFORE is invalid'
    confirm_epoch=$(date -u -d "$confirmation" +%s) || fail 'CONFIRM_BEFORE is invalid'
    now_epoch=$(date -u +%s)
    (( confirm_epoch <= now_epoch )) || fail 'CONFIRM_BEFORE must not be in the future'
    local_cutoff=$(( now_epoch - LOCAL_RETENTION_DAYS * 86400 ))
    replica_cutoff=$(( now_epoch - REPLICA_RETENTION_DAYS * 86400 ))
    shopt -s nullglob
    for path in "$REPLICA_DIRECTORY"/*.index; do
        name=${path##*/}
        name=${name%.index}
        validate_backup_name "$name"
        authenticate_backup "$REPLICA_DIRECTORY" "$name" replication
        created_epoch=$(date -u -d "$INDEX_CREATED_AT" +%s) || fail 'authenticated backup timestamp is invalid'
        records+=("$created_epoch $name")
    done
    shopt -u nullglob
    if (( ${#records[@]} == 0 )); then
        printf 'local_pruned=0\n'
        printf 'replica_destroyed=0\n'
        printf 'minimum_retained=%s\n' "$MIN_RETAINED_BACKUPS"
        return
    fi
    mapfile -t records < <(printf '%s\n' "${records[@]}" | sort -rn)
    for record in "${records[@]}"; do
        created_epoch=${record%% *}
        name=${record#* }
        if (( retained < MIN_RETAINED_BACKUPS )); then
            retained=$(( retained + 1 ))
            continue
        fi
        (( created_epoch < confirm_epoch )) || continue
        verify_bundle "$REPLICA_DIRECTORY" "$name" replication
        archive_hash=$INDEX_ARCHIVE_HASH
        manifest_hash=$INDEX_MANIFEST_HASH
        index_hash=$(sha256sum "$INDEX_PATH" | awk '{print $1}')
        if (( created_epoch < local_cutoff )); then
            if remove_local_bundle "$name"; then
                local_pruned=$(( local_pruned + 1 ))
            fi
        fi
        if (( created_epoch < replica_cutoff )); then
            write_tombstone "$name" "$archive_hash" "$manifest_hash" "$index_hash" "$INDEX_CREATED_AT"
            if remove_local_bundle "$name"; then
                local_pruned=$(( local_pruned + 1 ))
            fi
            remove_replica_bundle "$name"
            replica_destroyed=$(( replica_destroyed + 1 ))
        fi
    done
    printf 'local_pruned=%s\n' "$local_pruned"
    printf 'replica_destroyed=%s\n' "$replica_destroyed"
    printf 'minimum_retained=%s\n' "$MIN_RETAINED_BACKUPS"
}

main() {
    validate_schema
    validate_database_role
    [[ $SKIP_SAFETY_BACKUP == true || $SKIP_SAFETY_BACKUP == false ]] \
        || fail 'invalid SKIP_SAFETY_BACKUP'
    validate_storage
    umask 077
    case ${1:-} in
        schema-exists)
            load_database_url
            schema_exists || exit 3
            ;;
        backup)
            load_database_url
            load_recipient
            schema_exists || fail 'database schema does not exist'
            create_backup "${BACKUP_NAME:-}"
            ;;
        replicate)
            replicate_backup "${BACKUP_NAME:-}"
            printf 'replicated=%s\n' "$BACKUP_NAME"
            ;;
        fetch)
            fetch_backup "${BACKUP_NAME:-}"
            ;;
        verify)
            verify_backup "${BACKUP_NAME:-}"
            printf 'verified=%s\n' "$BACKUP_NAME"
            ;;
        verify-deep)
            load_identity
            verify_backup "${BACKUP_NAME:-}"
            authenticate_backup "$BACKUP_DIRECTORY" "$BACKUP_NAME" replicated
            printf 'deep_verified=%s\n' "$BACKUP_NAME"
            ;;
        restore)
            load_database_url
            load_identity
            restore_backup "${BACKUP_NAME:-}"
            ;;
        prune)
            load_identity
            prune_backups
            ;;
        *)
            fail 'usage: xs-db-tools schema-exists|backup|replicate|fetch|verify|verify-deep|restore|prune'
            ;;
    esac
}

main "$@"
