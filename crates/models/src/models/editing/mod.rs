//! Editing process.
//!
//! The process of producing a book can usually be divided into distinct stages
//! with a clear ordering. Sometimes an entire book will follow this process,
//! but often it will be divided into smaller parts, each of which will follow
//! the process in relative isolation from other parts. We call any such ordered
//! sequence of stages or steps, from before the work began until publication,
//! an _editing process_. Editing processes in this system are implemented for
//! the smallest unit of content we support, a [`Document`][Document].
//!
//! [Document]: ../document/struct.Document.html
//!
//! ## Anatomy of an editing process
//!
//! An editing process is, in essence, a rooted directed multidigraph with loops,
//! in which nodes represent different stages (steps) of edition, and edges
//! represent possible transition between these stages. Editing process must
//! have exactly one starting stage and at least one ending stage.
//!
//! When work on a particular document begins it automatically enters first
//! stage of the editing process assigned to it. After that stages are only
//! changed manually by a user, provided all conditions are met. Once a document
//! enters one of the final stages the editing process is automatically
//! concluded.
//!
//! Stages of an editing process specify who can access a document and what can
//! they do with it, and define a set of conditions a document must fulfill
//! before it can be advanced to a next stage. They do not however dictate what
//! should happen during that stage; they are intended to help in organizing and
//! managing the editing process through automation, but not to faithfully
//! represent what is really happening during the process. In reality a single
//! stage may correspond to multiple actual steps and _vice versa_.
//!
//! What can be done with a document at a particular stage, and by which user,
//! is described by _slots_. Slots are an abstract description of users used
//! when designing an editing process. When a document enters an editing process
//! those slots are filled in with real users, and afterwards when the editing
//! process references a slot in relation to this document, it will mean one
//! of those users. How slots are filled in is described in more detail in
//! section [Filling slots](#filling-slots).
//!
//! ### Filling slots
//!
//! When a document enters an editing step system will grant various permissions
//! to users occupying slots mentioned in that step. If any of those slots
//! is not yet occupied system will attempt to fill it. There are three ways
//! a slot can be filled:
//!
//! - *Manual*: user will manually select with whom to fill unoccupied slots in
//!   a step when advancing a document to said step;
//!
//! - *Automatic*: the system will automatically select a user to fill the slot;
//!
//! - *Voluntary*: the slot will remain unoccupied until a user voluntarily
//!   fills it.
//!
//! It is possible to limit who can fill a slot to specific role.
//!
//! A single user may occupy multiple slots.
//!
//! ### Slot permissions
//!
//! Slots control what users can do with a document at each stage through
//! permissions. Each slot can grant multiple permissions, and permissions can
//! be granted to multiple slots (with exceptions). Some permissions are
//! exclusive (they cannot be granted if another permission is), and some imply
//! other permissions.
//!
//! Currently existing permissions are:
//!
//! - *Viewing*: user can view the document during this stage.
//!
//! - *Editing*: user can make changes to the document.
//!
//!   This permission can only be granted to one slot. This permission cannot be
//!   granted if _proposing changes_ is also granted.
//!
//! - *Propose changes*: user can't directly edit the document, but they can
//!   propose changes to it.
//!
//!   This permission can only be granted to one slot. This permission cannot be
//!   granted if _editing_ is also granted.
//!
//! - *Accept changes*: user can accept changes proposed by a user with
//!   _proposing changes_ permission.
//!
//! ### Advancing document through stages
//!
//! A possible transition between stages is called a _link_. Links are
//! directional, that is one of their ends is considered source and the other
//! destination of the link, and documents can only traverse them from source
//! to destination, not in the opposite direction.
//!
//! TODO: more
//!
//! ## How editing processes fit into overall system
//!
//! The editing process operates on [`Document`s][Document], which don't exist
//! as standalone entities within the system. Instead, the system operates on
//! [`Module`s][Module], which are published and unchanging versions of
//! a document, and [`Draft`s][Draft], which are working versions of modules and
//! not publicly available. In other words modules are primary versions
//! of documents, and drafts are their versions during an editing process.
//!
//! When an editing process is started for a module, a draft of it is created
//! automatically, and process then operates in that draft. Once the process is
//! successfully concluded (reaches one of its final stages without being
//! aborted) that draft automatically becomes a new versions of the original
//! module.
//!
//! [Draft]: ../draft/struct.Draft.html
//! [Module]: ../module/struct.Module.html
//!
//! ## Versioning processes

mod link;
mod process;
mod slot;
mod step;
mod version;

pub mod structure;

pub use self::{
    link::Link,
    process::Process,
    slot::{FillSlotError, Slot},
    step::Step,
    version::{CreateVersionError, Version},
};
