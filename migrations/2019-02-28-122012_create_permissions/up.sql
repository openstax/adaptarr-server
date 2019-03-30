alter table users
add column permissions integer not null default 0;

alter table sessions
add column permissions integer not null default 0;

alter table sessions
rename column is_super to is_elevated;

-- Since we are switching to a permission-based model, and we don't know which
-- permissions a given session should have, we just drop all sessions.
delete from sessions;

-- Ensure existing superusers have all permissions (-1 is a signed 32 bit
-- number with all bits set).
update users
set permissions = -1
where is_super = true;
