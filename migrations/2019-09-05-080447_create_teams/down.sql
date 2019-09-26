-- Re-add roles to users

alter table users
add column role integer references roles(id);

update users
set role = team_members.role
from team_members
where users.id = team_members.user;

-- Re-calculate role and user permissions

update roles
set permissions = roles.permissions & 2031792;

with
    permissions as (
        select
            team_members.user as user,
            bit_and(team_members.permissions & 2031792) as permissions
        from team_members
        group by team_members.user
    )
update users
set permissions = users.permissions & 4103 | permissions.permissions
from permissions
where users.id = permissions.user;

-- Invalidate all existing invitations. Drop foreign keys on teams and users,
-- and team permissions.

delete from invites;

alter table invites
drop column team,
drop column permissions,
drop column "user";

-- Drop foreign key on teams from remaining tables

alter table modules
drop column team;

alter table books
drop column team;

alter table drafts
drop column team;

alter table roles
drop constraint roles_name_team_key,
drop column team,
add unique(name);

alter table edit_processes
drop column team;

alter table resources
drop column team;

-- Drop new tables

drop table team_members;
drop table teams;
