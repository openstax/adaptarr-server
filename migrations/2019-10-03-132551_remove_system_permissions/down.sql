alter table users
add permissions integer;

update users
set permissions = 0;

update users
set permissions = -1
where users.is_super = true;

alter table users
alter permissions set not null;

alter table sessions
add permissions integer;

update sessions
set permissions = 0;

alter table sessions
alter permissions set not null
