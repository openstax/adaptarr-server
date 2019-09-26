-- Create new tables

create table teams (
    id          serial  primary key,
    name        varchar not null
);

create table team_members (
    team        integer not null references teams(id),
    "user"      integer not null references users(id),
    permissions integer not null,
    role        integer references roles(id),

    primary key ("user", team)
);

-- Add foreign key for teams to other models. For now this column is nullable,
-- as there may be values already in those tables.

alter table modules
add column team integer references teams(id);

alter table books
add column team integer references teams(id);

alter table drafts
add column team integer references teams(id);

alter table roles
add column team integer references teams(id),
drop constraint roles_name_key,
add unique(name, team);

alter table edit_processes
add column team integer references teams(id);

alter table resources
add column team integer references teams(id);

-- Invalidate all existing invitations. Add foreign keys on teams and users,
-- and team permissions, to new invitations.

delete from invites;

alter table invites
add column team integer references teams(id),
add column permissions integer not null,
add column "user" integer references users(id);

-- Create a default team and use it to fill all newly created foreign keys. Also
-- add all existing users to the default team.

do $$ declare
    default_team integer;
begin
    insert into teams (name)
    values ('Default')
    returning id into default_team;

    update invites set team = default_team;
    update modules set team = default_team;
    update books set team = default_team;
    update drafts set team = default_team;
    update roles set team = default_team;
    update edit_processes set team = default_team;
    update resources set team = default_team;

    insert into team_members ("user", team, permissions, role)
    select
        users.id,
        default_team,
        users.permissions & 2035647,
        role
    from users;
end $$;

-- Update users' system permissions, and role permissions.

update users
set permissions = (permissions & 4103)
    | case is_super
        when true then 16777216
        when false then 0
    end;

update roles
set permissions = permissions & 2035647;

-- Change all newly created foreign keys on teams to be non null.

alter table invites
alter column team set not null;

alter table modules
alter column team set not null;

alter table books
alter column team set not null;

alter table drafts
alter column team set not null;

alter table roles
alter column team set not null;

alter table edit_processes
alter column team set not null;

alter table resources
alter column team set not null;

-- Remove role from users, it's now assigned on a per-team basis.

alter table users
drop column role;
