\set ON_ERROR_STOP on
\getenv app_password XS_DATABASE_APP_PASSWORD
\getenv migrator_password XS_DATABASE_MIGRATOR_PASSWORD

SELECT :'schema' ~ '^[a-z_][a-z0-9_]{0,62}$'
   AND :'owner_role' ~ '^[a-z_][a-z0-9_]{0,62}$'
   AND :'app_role' ~ '^[a-z_][a-z0-9_]{0,62}$'
   AND :'migrator_role' ~ '^[a-z_][a-z0-9_]{0,62}$'
   AND :'owner_role' <> :'app_role'
   AND :'owner_role' <> :'migrator_role'
   AND :'app_role' <> :'migrator_role'
   AND length(:'app_password') >= 32
   AND length(:'migrator_password') >= 32 AS inputs_valid
\gset
\if :inputs_valid
\else
SELECT 1 / 0;
\endif

BEGIN;

SELECT format('CREATE ROLE %I', :'owner_role')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'owner_role')
\gexec
SELECT format('CREATE ROLE %I', :'app_role')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'app_role')
\gexec
SELECT format('CREATE ROLE %I', :'migrator_role')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'migrator_role')
\gexec

ALTER ROLE :"owner_role" NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE :"app_role" LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS INHERIT CONNECTION LIMIT 20;
ALTER ROLE :"migrator_role" LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS INHERIT CONNECTION LIMIT 2;
SELECT format('ALTER ROLE %I PASSWORD %L', :'app_role', :'app_password')
\gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'migrator_role', :'migrator_password')
\gexec

SELECT format('REVOKE %I FROM %I', :'owner_role', :'app_role')
WHERE pg_has_role(:'app_role', :'owner_role', 'MEMBER')
\gexec
SELECT format('GRANT %I TO %I', :'owner_role', :'migrator_role')
WHERE NOT pg_has_role(:'migrator_role', :'owner_role', 'MEMBER')
\gexec

SELECT format('CREATE SCHEMA %I AUTHORIZATION %I', :'schema', :'owner_role')
WHERE NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = :'schema')
\gexec
ALTER SCHEMA :"schema" OWNER TO :"owner_role";

SELECT format(
    CASE c.relkind
        WHEN 'S' THEN 'ALTER SEQUENCE %I.%I OWNER TO %I'
        WHEN 'v' THEN 'ALTER VIEW %I.%I OWNER TO %I'
        WHEN 'm' THEN 'ALTER MATERIALIZED VIEW %I.%I OWNER TO %I'
        WHEN 'f' THEN 'ALTER FOREIGN TABLE %I.%I OWNER TO %I'
        ELSE 'ALTER TABLE %I.%I OWNER TO %I'
    END,
    n.nspname,
    c.relname,
    :'owner_role'
)
FROM pg_class AS c
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE n.nspname = :'schema'
  AND c.relkind IN ('r', 'p', 'S', 'v', 'm', 'f')
  AND (
      c.relkind <> 'S'
      OR NOT EXISTS (
          SELECT 1
          FROM pg_depend AS d
          WHERE d.objid = c.oid
            AND d.deptype IN ('a', 'i')
      )
  )
ORDER BY c.relkind, c.relname
\gexec

SELECT format(
    'ALTER FUNCTION %I.%I(%s) OWNER TO %I',
    n.nspname,
    p.proname,
    pg_get_function_identity_arguments(p.oid),
    :'owner_role'
)
FROM pg_proc AS p
JOIN pg_namespace AS n ON n.oid = p.pronamespace
WHERE n.nspname = :'schema'
ORDER BY p.proname, p.oid
\gexec

SELECT format('REVOKE CREATE ON DATABASE %I FROM PUBLIC', current_database())
\gexec
SELECT format('GRANT CONNECT, CREATE ON DATABASE %I TO %I', current_database(), :'owner_role')
\gexec
SELECT format('GRANT CONNECT ON DATABASE %I TO %I', current_database(), :'app_role')
\gexec
SELECT format('GRANT CONNECT ON DATABASE %I TO %I', current_database(), :'migrator_role')
\gexec
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
REVOKE ALL ON SCHEMA :"schema" FROM :"app_role";
GRANT USAGE ON SCHEMA :"schema" TO :"app_role";
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA :"schema" TO :"app_role";
REVOKE INSERT, UPDATE, DELETE ON TABLE :"schema"._sqlx_migrations FROM :"app_role";
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA :"schema" TO :"app_role";
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA :"schema" TO :"app_role";

ALTER DEFAULT PRIVILEGES FOR ROLE :"owner_role" IN SCHEMA :"schema"
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO :"app_role";
ALTER DEFAULT PRIVILEGES FOR ROLE :"owner_role" IN SCHEMA :"schema"
    GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO :"app_role";
ALTER DEFAULT PRIVILEGES FOR ROLE :"owner_role" IN SCHEMA :"schema"
    GRANT EXECUTE ON FUNCTIONS TO :"app_role";

COMMIT;

SELECT 'role_hardening_completed=yes';
