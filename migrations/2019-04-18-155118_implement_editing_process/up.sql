-- Create new tables

alter table drafts
drop constraint drafts_pkey,
add primary key (module);

create table draft_slots (
    draft   uuid    not null references drafts(module) on delete cascade,
    slot    integer not null references edit_process_slots(id),
    "user"  integer not null references users(id),

    primary key (draft, slot)
);

alter table drafts
add column step integer references edit_process_steps(id);

-- Create a new editing process mimicking the old behavior of drafts and migrate
-- drafts to it.

do $$ declare
    old_process_id integer;
    old_process_version integer;
    old_process_assignee integer;
    old_process_step integer;
    old_process_finish integer;
begin
    -- Create a new editing process to hold old drafts in

    set constraints all deferred;

    insert into edit_processes (name)
    values ('Old editing process')
    on conflict do nothing
    returning id into old_process_id;

    if old_process_id is not null then
        insert into edit_process_versions (process, start, version)
        values (old_process_id, 0, now())
        returning id into old_process_version;

        insert into edit_process_slots (process, name, autofill)
        values (old_process_version, 'Assignee', false)
        returning id into old_process_assignee;

        insert into edit_process_steps (process, name)
        values (old_process_version, 'Assigned')
        returning id into old_process_step;

        update edit_process_versions
        set start = old_process_step
        where id = old_process_version;

        insert into edit_process_step_slots (step, slot, permission)
        values (old_process_step, old_process_assignee, 'edit');

        insert into edit_process_steps (process, name)
        values (old_process_version, 'Finished')
        returning id into old_process_finish;

        insert into edit_process_links ("from", "to", name, slot)
        values (old_process_step, old_process_finish, 'Finish', old_process_assignee);
    else
        select id into old_process_id
        from edit_processes
        where name = 'Old editing process';

        select id into old_process_version
        from edit_process_versions
        where process = old_process_id;

        select id into old_process_assignee
        from edit_process_slots
        where process = old_process_version;

        select id into old_process_step
        from edit_process_steps
        where name = 'Assigned'
          and process = old_process_version;
    end if;

    set constraints all immediate;

    -- Move drafts from being user-owned to being in-process

    insert into draft_slots (draft, slot, "user")
    select drafts.module, old_process_assignee, drafts.user
    from drafts;

    update drafts set step = old_process_step;
end $$;

-- Finish creating new structures

alter table drafts
alter column step set not null;

-- Remove last remaining parts of old editing process.

alter table drafts
drop column "user";

alter table modules
drop column assignee;
