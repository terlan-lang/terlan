create table packages (
    name text primary key,
    inserted_at timestamptz not null default now()
);
