\set ON_ERROR_STOP on

SELECT current_user = :'app_role' AS runtime_role_matches,
       NOT rolsuper AS no_superuser,
       NOT rolcreatedb AS no_createdb,
       NOT rolcreaterole AS no_createrole,
       NOT rolreplication AS no_replication,
       NOT rolbypassrls AS no_bypassrls
FROM pg_roles
WHERE rolname = current_user
\gset

SELECT NOT pg_has_role(:'app_role', :'owner_role', 'MEMBER') AS not_owner_member,
       NOT has_database_privilege(:'app_role', current_database(), 'CREATE') AS no_database_create,
       has_schema_privilege(:'app_role', :'schema', 'USAGE') AS schema_usage,
       NOT has_schema_privilege(:'app_role', :'schema', 'CREATE') AS no_schema_create,
       has_table_privilege(:'app_role', format('%I.networks', :'schema'), 'SELECT') AS table_select,
       has_table_privilege(:'app_role', format('%I.networks', :'schema'), 'INSERT') AS table_insert,
       has_table_privilege(:'app_role', format('%I.networks', :'schema'), 'UPDATE') AS table_update,
       has_table_privilege(:'app_role', format('%I.networks', :'schema'), 'DELETE') AS table_delete,
       has_table_privilege(:'app_role', format('%I._sqlx_migrations', :'schema'), 'SELECT') AS migration_table_select,
       NOT has_table_privilege(:'app_role', format('%I._sqlx_migrations', :'schema'), 'INSERT') AS no_migration_table_insert,
       NOT has_table_privilege(:'app_role', format('%I._sqlx_migrations', :'schema'), 'UPDATE') AS no_migration_table_update,
       NOT has_table_privilege(:'app_role', format('%I._sqlx_migrations', :'schema'), 'DELETE') AS no_migration_table_delete,
       NOT has_table_privilege(:'app_role', 'pg_catalog.pg_authid', 'SELECT') AS no_auth_catalog
\gset

SELECT bool_and(c.relowner = owner.oid) AS objects_owned_by_owner
FROM pg_class AS c
JOIN pg_namespace AS n ON n.oid = c.relnamespace
JOIN pg_roles AS owner ON owner.rolname = :'owner_role'
WHERE n.nspname = :'schema'
  AND c.relkind IN ('r', 'p', 'S', 'v', 'm', 'f')
\gset

SELECT bool_and(p.proowner = owner.oid) AS functions_owned_by_owner
FROM pg_proc AS p
JOIN pg_namespace AS n ON n.oid = p.pronamespace
JOIN pg_roles AS owner ON owner.rolname = :'owner_role'
WHERE n.nspname = :'schema'
\gset

\if :runtime_role_matches
\else
SELECT 1 / 0;
\endif
\if :no_superuser
\else
SELECT 1 / 0;
\endif
\if :no_createdb
\else
SELECT 1 / 0;
\endif
\if :no_createrole
\else
SELECT 1 / 0;
\endif
\if :no_replication
\else
SELECT 1 / 0;
\endif
\if :no_bypassrls
\else
SELECT 1 / 0;
\endif
\if :not_owner_member
\else
SELECT 1 / 0;
\endif
\if :no_database_create
\else
SELECT 1 / 0;
\endif
\if :schema_usage
\else
SELECT 1 / 0;
\endif
\if :no_schema_create
\else
SELECT 1 / 0;
\endif
\if :table_select
\else
SELECT 1 / 0;
\endif
\if :table_insert
\else
SELECT 1 / 0;
\endif
\if :table_update
\else
SELECT 1 / 0;
\endif
\if :table_delete
\else
SELECT 1 / 0;
\endif
\if :migration_table_select
\else
SELECT 1 / 0;
\endif
\if :no_migration_table_insert
\else
SELECT 1 / 0;
\endif
\if :no_migration_table_update
\else
SELECT 1 / 0;
\endif
\if :no_migration_table_delete
\else
SELECT 1 / 0;
\endif
\if :no_auth_catalog
\else
SELECT 1 / 0;
\endif
\if :objects_owned_by_owner
\else
SELECT 1 / 0;
\endif
\if :functions_owned_by_owner
\else
SELECT 1 / 0;
\endif

SELECT 'least_privilege_verification=pass';
