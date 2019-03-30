alter table users
drop column permissions;

alter table sessions
drop column permissions;

alter table sessions
rename column is_elevated to is_super;

-- Since we are switching back from a permission-based model, and we don't know
-- which permissions should map to elevated sessions and which to normal
-- sessions, we just drop all sessions.
delete from sessions;
