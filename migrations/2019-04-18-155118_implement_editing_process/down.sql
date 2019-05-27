-- Add users back to drafts

alter table drafts
add column "user" integer references users(id);

-- We don't know which slot to treat as draft's new owner, so we just pick any
-- of them.
update drafts
set "user" = draft_slots."user"
from draft_slots
where drafts.module = draft_slots.draft;

alter table drafts
alter column "user" set not null;

-- Add assignees back, and assign modules to current draft owners

alter table modules
add column assignee integer references users(id);

update modules
set assignee = drafts."user"
from drafts
where modules.id = drafts.module;

-- Remove step tracking from drafts

alter table drafts
drop column step;

drop table draft_slots;

-- Make (module, user) primary key again

alter table drafts
drop constraint drafts_pkey,
add primary key (module, "user");
