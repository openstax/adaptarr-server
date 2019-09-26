alter table sessions
alter column expires set data type timestamp without time zone,
alter column last_used set data type timestamp without time zone;

alter table invites
alter column expires set data type timestamp without time zone;

alter table password_reset_tokens
alter column expires set data type timestamp without time zone;

alter table module_versions
alter column version set data type timestamp without time zone;

alter table events
alter column timestamp set data type timestamp without time zone;

alter table edit_process_versions
alter column version set data type timestamp without time zone;

alter table audit_log
alter column timestamp set data type timestamp without time zone;
