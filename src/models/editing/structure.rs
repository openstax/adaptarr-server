pub use crate::db::types::SlotPermission;

use self::ValidateStructureError::*;

#[derive(Debug, Deserialize, Serialize)]
pub struct Process {
    /// Process's name.
    pub name: String,
    /// Index of the initial step.
    pub start: usize,
    /// Slots defined for this process.
    pub slots: Vec<Slot>,
    /// Steps in this process.
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Slot {
    /// Database ID of this slot.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub role: Option<i32>,
    #[serde(default)]
    pub autofill: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Step {
    /// Database ID of this step.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub slots: Vec<StepSlot>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StepSlot {
    pub slot: usize,
    pub permission: SlotPermission,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Link {
    pub name: String,
    pub to: usize,
    pub slot: usize,
}

/// Result of validation.
#[derive(Debug)]
pub struct Validation {
}

pub fn validate(process: &Process) -> Result<Validation, ValidateStructureError> {
    if process.name.is_empty() {
        return Err(ValidateStructureError::EmptyProcessName);
    }

    // Verify all IDs are correct.

    if process.start >= process.steps.len() {
        return Err(ValidateStructureError::InvalidStartStep(
            process.start, process.steps.len()));
    }

    for (stepid, step) in process.steps.iter().enumerate() {
        for slot in &step.slots {
            if slot.slot >= process.slots.len() {
                return Err(InvalidStepSlot {
                    step: stepid,
                    slot: slot.slot,
                    permission: slot.permission,
                    total: process.slots.len(),
                });
            }
        }

        for (linkid, link) in step.links.iter().enumerate() {
            if link.to >= process.steps.len() {
                return Err(InvalidLinkTarget {
                    step: stepid,
                    link: linkid,
                    target: link.to,
                    total: process.steps.len(),
                });
            }

            if link.slot >= process.slots.len() {
                return Err(InvalidLinkSlot {
                    step: stepid,
                    link: linkid,
                    slot: link.slot,
                    total: process.slots.len(),
                })
            }

            if !step.slots.iter().any(|slot| slot.slot == link.slot) {
                return Err(UnusableLink {
                    step: stepid,
                    link: linkid,
                    slot: link.slot,
                });
            }
        }
    }

    // Verify there are no conflicting permissions

    for (stepid, step) in process.steps.iter().enumerate() {
        let mut has_editing = false;
        let mut has_propose = false;
        let mut has_accept = false;

        for slot in &step.slots {
            match slot.permission {
                SlotPermission::View => (),
                SlotPermission::Edit => {
                    if has_editing {
                        return Err(PermissionDuplication {
                            step: stepid,
                            permission: slot.permission,
                        });
                    }
                    has_editing = true;
                }
                SlotPermission::ProposeChanges => {
                    if has_editing {
                        return Err(PermissionDuplication {
                            step: stepid,
                            permission: slot.permission,
                        });
                    }
                    has_propose = true;
                }
                SlotPermission::AcceptChanges => has_accept = true,
            }
        }

        if has_editing && has_propose {
            return Err(ConflictingPermissions {
                step: stepid,
                permission_a: SlotPermission::Edit,
                permission_b: SlotPermission::ProposeChanges,
            });
        }

        if has_propose && !has_accept {
            return Err(MissingRequiredPermission {
                step: stepid,
                requirer: SlotPermission::ProposeChanges,
                requiree: SlotPermission::AcceptChanges,
            });
        }

        if has_accept && !has_propose {
            return Err(MissingRequiredPermission {
                step: stepid,
                requirer: SlotPermission::AcceptChanges,
                requiree: SlotPermission::ProposeChanges,
            });
        }
    }

    // Verify all steps are reachable from the initial step.

    let mut reachable = vec![false; process.steps.len()];
    let mut stack = vec![process.start];

    while let Some(node) = stack.pop() {
        if reachable[node] {
            continue;
        }

        reachable[node] = true;

        for link in &process.steps[node].links {
            stack.push(link.to);
        }
    }

    if let Some(node) = reachable.iter().position(|reachable| !reachable) {
        return Err(UnreachableState(node));
    }

    // Verify that the initial step is not also a final step.

    let final_steps: Vec<usize> = process.steps.iter()
        .enumerate()
        .filter(|(_, step)| step.links.is_empty())
        .map(|(inx, _)| inx)
        .collect();

    if final_steps.iter().any(|&f| f == process.start) {
        return Err(ValidateStructureError::StartIsFinal);
    }

    // Verify there's a patch from every step to a final step.

    let mut reachable = vec![false; process.steps.len()];
    let mut stack = final_steps;

    let mut links = vec![Vec::new(); process.steps.len()];
    for (inx, step) in process.steps.iter().enumerate() {
        for link in &step.links {
            links[link.to].push(inx);
        }
    }

    while let Some(node) = stack.pop() {
        if reachable[node] {
            continue;
        }

        reachable[node] = true;

        stack.extend(&links[node]);
    }

    if let Some(node) = reachable.iter().position(|reachable| !reachable) {
        return Err(IsolatedStep(node));
    }

    Ok(Validation {})
}

#[derive(Debug, Fail)]
pub enum ValidateStructureError {
    /// Process name is empty.
    #[fail(display = "Process's name cannot be empty")]
    EmptyProcessName,
    /// Description names start step with ID greater than total number of steps.
    #[fail(display = "Start step's ID {} exceeds total number of steps {}", _0, _1)]
    InvalidStartStep(usize, usize),
    /// Step description gives permission to a slot with ID greater than total
    /// number of slots.
    #[fail(
        display =
            "Step {} grants permission {} to slot {} whose ID exceeds total \
            number of slots {}",
        step,
        permission,
        slot,
        total,
    )]
    InvalidStepSlot {
        /// Offending step's ID.
        step: usize,
        /// Offending slot's ID.
        slot: usize,
        /// Permission granted to offending slot.
        permission: SlotPermission,
        /// Total number of slots.
        total: usize,
    },
    /// Link description targets a step with ID greater than total number
    /// of steps.
    #[fail(
        display =
            "Link {} of step {} targets step {} whose ID exceeds total number \
            of steps {}",
        link,
        step,
        target,
        total,
    )]
    InvalidLinkTarget {
        /// Offending step's ID.
        step: usize,
        /// Offending link's ID.
        link: usize,
        /// Offending target ID.
        target: usize,
        /// Total number of steps.
        total: usize,
    },
    /// Link description references a slot with ID greater than total number
    /// of slots.
    #[fail(
        display =
            "Link {} of step {} references slot {} whose ID exceeds total \
            number of slots {}",
        link,
        step,
        slot,
        total,
    )]
    InvalidLinkSlot {
        /// Offending step's ID.
        step: usize,
        /// Offending link's ID.
        link: usize,
        /// Offending slot ID.
        slot: usize,
        /// Total number of slots.
        total: usize,
    },
    /// Link description references a slot which has no permissions in that step.
    #[fail(
        display =
            "Link {} of step {} references slot {} which is granted \
            no permissions in that step",
        link,
        step,
        slot,
    )]
    UnusableLink {
        /// Offending step's ID.
        step: usize,
        /// Offending link's ID.
        link: usize,
        /// Offending slot ID.
        slot: usize,
    },
    /// A limited permission is granted to many slots in a step.
    #[fail(
        display =
            "Step {} grants permission {} to multiple slots, but it can only \
            be granted to one",
        step,
        permission,
    )]
    PermissionDuplication {
        /// Offending step's ID.
        step: usize,
        /// Offending permission.
        permission: SlotPermission,
    },
    /// Step grants conflicting permissions.
    #[fail(
        display =
            "Step {} grants permissions {} and {}, but they cannot both \
            be granted",
        step,
        permission_a,
        permission_b,
    )]
    ConflictingPermissions {
        /// Offending step's ID.
        step: usize,
        /// First conflicting permission.
        permission_a: SlotPermission,
        /// Second conflicting permission.
        permission_b: SlotPermission,
    },
    /// Step grants a permission, but not a permission it requires.
    #[fail(
        display =
            "Step {} grants permission {}, but not permission {} required by it",
        step,
        requirer,
        requiree,
    )]
    MissingRequiredPermission {
        /// Offending step's ID.
        step: usize,
        /// Permission which requires another.
        requirer: SlotPermission,
        /// Permission which is required but not granted.
        requiree: SlotPermission,
    },
    /// Description contains a step which is not reachable from the initial step.
    #[fail(display = "Step {} is not reachable from the initial step", _0)]
    UnreachableState(usize),
    /// Description contains a step from which no final step can be reached.
    #[fail(display = "Step {} is isolated; no final step can be reached from it", _0)]
    IsolatedStep(usize),
    /// The initial step is also a final step.
    #[fail(display = "Start step cannot also be a final step")]
    StartIsFinal,
}
