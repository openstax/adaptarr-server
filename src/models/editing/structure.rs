use failure::Fail;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::db::types::SlotPermission;

use self::ValidateStructureError::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StepSlot {
    pub slot: usize,
    pub permission: SlotPermission,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Link {
    pub name: String,
    pub to: usize,
    pub slot: usize,
}

/// Result of validation.
#[derive(Debug, Eq, PartialEq)]
pub struct Validation {
}

pub fn validate(process: &Process) -> Result<Validation, ValidateStructureError> {
    // Verify there are no empty or duplicate names.

    if process.name.is_empty() {
        return Err(ValidateStructureError::EmptyProcessName);
    }

    let mut slots = HashMap::new();

    for (slotid, slot) in process.slots.iter().enumerate() {
        if slot.name.is_empty() {
            return Err(ValidateStructureError::EmptySlotName(slotid));
        }

        if let Some(inx) = slots.get(&slot.name) {
            return Err(ValidateStructureError::DuplicateSlotName(slotid, *inx));
        }

        slots.insert(&slot.name, slotid);
    }

    let mut steps = HashMap::new();

    for (stepid, step) in process.steps.iter().enumerate() {
        if step.name.is_empty() {
            return Err(ValidateStructureError::EmptyStepName(stepid));
        }

        if let Some(inx) = steps.get(&step.name) {
            return Err(ValidateStructureError::DuplicateStepName(stepid, *inx));
        }

        steps.insert(&step.name, stepid);

        let mut links = HashMap::new();

        for (linkid, link) in step.links.iter().enumerate() {
            if link.name.is_empty() {
                return Err(ValidateStructureError::EmptyLinkName(stepid, linkid));
            }

            if let Some(inx) = links.get(&link.name) {
                return Err(ValidateStructureError::DuplicateLinkName {
                    step: stepid,
                    link: linkid,
                    previous: *inx,
                });
            }

            links.insert(&link.name, linkid);
        }
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

            if link.to == stepid {
                return Err(LoopedLink {
                    step: stepid,
                    link: linkid,
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
                    if has_propose {
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

#[derive(Debug, Eq, Fail, PartialEq)]
pub enum ValidateStructureError {
    /// Process name is empty.
    #[fail(display = "Process's name cannot be empty")]
    EmptyProcessName,
    /// Name of a slot is empty.
    #[fail(display = "Slot {}'s name cannot be empty", _0)]
    EmptySlotName(usize),
    /// Name of a step is empty.
    #[fail(display = "Step {}'s name cannot be empty", _0)]
    EmptyStepName(usize),
    /// Name of a link is empty.
    #[fail(display = "Step {}'s link {}'s name cannot be empty", _0, _1)]
    EmptyLinkName(usize, usize),
    /// Description names start step with ID greater than total number of steps.
    #[fail(display = "Start step's ID {} exceeds total number of steps {}", _0, _1)]
    InvalidStartStep(usize, usize),
    /// Process contains two slots with the same name.
    #[fail(display = "Slot {} has the same name as slot {}", _0, _1)]
    DuplicateSlotName(usize, usize),
    /// Process contains two steps with the same name.
    #[fail(display = "Step {} has the same name as step {}", _0, _1)]
    DuplicateStepName(usize, usize),
    /// Step contains two links with the same name.
    #[fail(
        display = "Link {} in step {} has the same name as link {}",
        link,
        step,
        previous,
    )]
    DuplicateLinkName {
        /// Offending step's ID.
        step: usize,
        /// Offending link's ID.
        link: usize,
        /// Previous link with the same name.
        previous: usize,
    },
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
    /// Link description specifies its origin as target.
    #[fail(display = "Link {} of step {} targets itself", link, step)]
    LoopedLink {
        /// Offending step's ID.
        step: usize,
        /// Offending link's ID.
        link: usize,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        let good = Process {
            name: "Process".into(),
            start: 0,
            slots: vec![
                Slot {
                    id: 0,
                    name: "Slot".into(),
                    role: None,
                    autofill: false,
                },
            ],
            steps: vec![
                Step {
                    id: 0,
                    name: "Start".into(),
                    slots: vec![
                        StepSlot {
                            slot: 0,
                            permission: SlotPermission::Edit,
                        },
                    ],
                    links: vec![
                        Link {
                            name: "Link".into(),
                            slot: 0,
                            to: 1,
                        },
                    ],
                },
                Step {
                    id: 0,
                    name: "End".into(),
                    slots: vec![],
                    links: vec![],
                },
            ],
        };

        assert_eq!(validate(&good), Ok(Validation {}));

        let mut test = good.clone();
        test.slots[0].name = "".into();
        assert_eq!(validate(&test), Err(ValidateStructureError::EmptySlotName(0)));

        let mut test = good.clone();
        test.steps[0].name = "".into();
        assert_eq!(validate(&test), Err(ValidateStructureError::EmptyStepName(0)));

        let mut test = good.clone();
        test.steps[0].links[0].name = "".into();
        assert_eq!(validate(&test), Err(ValidateStructureError::EmptyLinkName(0, 0)));

        let mut test = good.clone();
        test.slots.push(Slot {
            id: 1,
            name: "Slot".into(),
            role: None,
            autofill: false,
        });
        assert_eq!(
            validate(&test), Err(ValidateStructureError::DuplicateSlotName(1, 0)));

        let mut test = good.clone();
        test.steps[1].name = "Start".into();
        assert_eq!(
            validate(&test), Err(ValidateStructureError::DuplicateStepName(1, 0)));

        let mut test = good.clone();
        test.steps[0].links.push(Link {
            name: "Link".into(),
            slot: 0,
            to: 1,
        });
        assert_eq!(
            validate(&test),
            Err(ValidateStructureError::DuplicateLinkName {
                step: 0,
                link: 1,
                previous: 0,
            }),
        );

        assert_eq!(validate(&Process {
            name: "".into(),
            .. good.clone()
        }), Err(ValidateStructureError::EmptyProcessName));

        assert_eq!(validate(&Process {
            steps: vec![],
            .. good.clone()
        }), Err(ValidateStructureError::InvalidStartStep(0, 0)));

        let mut test = good.clone();
        test.steps[0].links.clear();
        test.steps.remove(1);
        assert_eq!(validate(&test), Err(ValidateStructureError::StartIsFinal));

        let mut test = good.clone();
        test.steps[0].links.clear();
        assert_eq!(
            validate(&test), Err(ValidateStructureError::UnreachableState(1)));

        let mut test = good.clone();
        test.steps[0].links[0].slot = 3;
        assert_eq!(validate(&test), Err(ValidateStructureError::InvalidLinkSlot {
            step: 0,
            link: 0,
            slot: 3,
            total: 1,
        }));

        let mut test = good.clone();
        test.steps[0].slots.clear();
        assert_eq!(validate(&test), Err(ValidateStructureError::UnusableLink {
            step: 0,
            link: 0,
            slot: 0,
        }));

        let mut test = good.clone();
        test.steps[0].links.push(Link {
            name: "Another link".into(),
            slot: 0,
            to: 0,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::LoopedLink {
            step: 0,
            link: 1,
        }));

        let mut test = good.clone();
        test.steps[0].links.push(Link {
            name: "Another link".into(),
            slot: 0,
            to: 3,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::InvalidLinkTarget {
            step: 0,
            link: 1,
            target: 3,
            total: 2,
        }));

        let mut test = good.clone();
        test.steps[0].slots.push(StepSlot {
            slot: 3,
            permission: SlotPermission::View,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::InvalidStepSlot {
            step: 0,
            slot: 3,
            permission: SlotPermission::View,
            total: 1,
        }));

        let mut test = good.clone();
        test.slots.push(Slot {
            id: 1,
            name: "Another".into(),
            role: None,
            autofill: false,
        });
        test.steps[0].slots.push(StepSlot {
            slot: 1,
            permission: SlotPermission::Edit,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::PermissionDuplication {
            step: 0,
            permission: SlotPermission::Edit,
        }));

        let mut test = good.clone();
        test.steps[0].slots.push(StepSlot {
            slot: 0,
            permission: SlotPermission::ProposeChanges,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::ConflictingPermissions {
            step: 0,
            permission_a: SlotPermission::Edit,
            permission_b: SlotPermission::ProposeChanges,
        }));

        let mut test = good.clone();
        test.steps[0].slots[0].permission = SlotPermission::AcceptChanges;
        assert_eq!(validate(&test), Err(ValidateStructureError::MissingRequiredPermission {
            step: 0,
            requirer: SlotPermission::AcceptChanges,
            requiree: SlotPermission::ProposeChanges,
        }));

        let mut test = good.clone();
        test.steps.push(Step {
            id: 2,
            name: "Isolated A".into(),
            slots: vec![
                StepSlot {
                    slot: 0,
                    permission: SlotPermission::View,
                },
            ],
            links: vec![
                Link {
                    name: "Link".into(),
                    slot: 0,
                    to: 3,
                },
            ],
        });
        test.steps.push(Step {
            id: 3,
            name: "Isolated B".into(),
            slots: vec![
                StepSlot {
                    slot: 0,
                    permission: SlotPermission::View,
                },
            ],
            links: vec![
                Link {
                    name: "Link".into(),
                    slot: 0,
                    to: 2,
                },
            ],
        });
        test.steps[0].links.push(Link {
            name: "Another link".into(),
            slot: 0,
            to: 2,
        });
        assert_eq!(validate(&test), Err(ValidateStructureError::IsolatedStep(2)));
    }
}
