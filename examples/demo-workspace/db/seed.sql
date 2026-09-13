-- Seed data for db/demo.sqlite (crates/vix-db/spec/index.md: DB Workbench).
-- Regenerate the .sqlite file from this script with db/regenerate.sh.

DROP TABLE IF EXISTS people;
DROP TABLE IF EXISTS tasks;

CREATE TABLE people (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    role  TEXT NOT NULL,
    score INTEGER NOT NULL
);

INSERT INTO people (id, name, role, score) VALUES
    (1, 'Alice', 'engineer', 92),
    (2, 'Bob',   'designer', 81),
    (3, 'Carol', 'engineer', 88),
    (4, 'Dave',  'manager',  75);

CREATE TABLE tasks (
    id         INTEGER PRIMARY KEY,
    person_id  INTEGER NOT NULL REFERENCES people(id),
    title      TEXT NOT NULL,
    done       INTEGER NOT NULL DEFAULT 0
);

INSERT INTO tasks (id, person_id, title, done) VALUES
    (1, 1, 'Fix word_counts lowercasing bug', 0),
    (2, 1, 'Review api.http examples',        1),
    (3, 3, 'Write demo.sqlite seed data',     1),
    (4, 4, 'Plan next tutorial chapter',      0);
