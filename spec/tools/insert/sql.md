# Tools: Insert: SQL

Inserts ready-to-edit PostgreSQL statements at the cursor (`App::insert_sql`).

- menu "Tools"
  - submenu "Insert"
    - submenu "SQL"
      - menuitem "Alter Role" -> insert an `ALTER ROLE … WITH CREATEDB;` statement.
      - menuitem "Create Extension" -> insert a commented list of common
        `CREATE EXTENSION IF NOT EXISTS …` statements (pgcrypto, pg_trgm,
        uuid-ossp, ltree, cube, …).
      - menuitem "Create Function" -> insert an `updated_at()` trigger function
        (`plpgsql`) that stamps `NEW.updated_at`.
      - menuitem "Create User" -> insert a `CREATE USER … WITH LOGIN ENCRYPTED
        PASSWORD …;` statement.
      - menuitem "Grant Create" -> insert `GRANT CREATE ON SCHEMA public TO …;`.
      - menuitem "Grant Usage" -> insert `GRANT USAGE ON SCHEMA public TO …;`.
      - menuitem "Create Table" -> insert a `CREATE TABLE items (…)` with an
        identity primary key, `created_at`/`updated_at` timestamps, an
        `updated_at` BEFORE UPDATE trigger, and a GIN trigram index.

The snippets are placeholders (e.g. the role `alice`, table `items`) meant to be
edited after insertion. The long ones live in the `SQL_CREATE_EXTENSION` and
`SQL_CREATE_TABLE` constants.
