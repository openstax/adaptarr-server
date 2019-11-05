//! Fine-grained control over actions a user can take.

use adaptarr_error::{ApiError, StatusCode, Value, Map};
use bitflags::bitflags;
use failure::Fail;
use serde::{de, ser::{self, SerializeSeq}};
use std::{borrow::Cow, fmt, marker::PhantomData};

pub trait PermissionBits: Copy + fmt::Debug + Sized + Send + Sync + 'static {
    fn empty() -> Self;

    /// Construct a new set of permissions from raw bits.
    fn from_bits(bits: i32) -> Option<Self>;

    fn from_str(s: &str) -> Option<Self>;

    /// Verify that all required permissions are present.
    ///
    /// This is the same check as `self.contains(permissions)`, but returns an
    /// [`ApiError`].
    fn require(self, permissions: Self) -> Result<(), RequirePermissionsError<Self>>;

    fn as_str(self) -> &'static str;

    fn insert(&mut self, permissions: Self);
}

#[derive(Debug, Fail)]
pub struct RequirePermissionsError<B: PermissionBits>(B);

impl<B: PermissionBits + ser::Serialize> ApiError for RequirePermissionsError<B> {
    fn status(&self) -> StatusCode { StatusCode::FORBIDDEN }

    fn code(&self) -> Option<Cow<str>> {
        Some(Cow::Borrowed("user:insufficient-permissions"))
    }

    fn data(&self) -> Option<Value> {
        let mut map = Map::default();
        map.insert(
            "permissions".to_string(),
            adaptarr_error::to_value(self.0).expect("serialization error"),
        );
        Some(map.into())
    }
}

impl<B: PermissionBits> fmt::Display for RequirePermissionsError<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Missing required permissions: {}", self.0.as_str())
    }
}

bitflags! {
    /// Permissions within a specific team.
    pub struct TeamPermissions: i32 {
        /// All currently allocated bits.
        const ALL_BITS = 0x001f_ff0f;
        /// Bits which used to name permissions, but those permissions were
        /// deprecated.
        const DEPRECATED_BITS = 0x0000_0000;
        /// All bits allocated for member management permissions.
        const MANAGE_MEMBERS_BITS = 0x0000_000f;
        /// Permission holder can add a member to the team.
        ///
        /// If the user to be added as a member already has an account, this
        /// permission is sufficient. Otherwise [`SystemPermissions::INVITE_USER`]
        /// is also required.
        const ADD_MEMBER = 0x0000_0001;
        /// Permission holder can remove a member from the team.
        const REMOVE_MEMBER = 0x0000_0002;
        /// Permission holder can change other member's permissions.
        const EDIT_MEMBER_PERMISSIONS = 0x0000_0004;
        /// Permission holder can change other member's roles.
        const ASSIGN_ROLE = 0x0000_0008;
        /// All bits allocated for content management permissions.
        const MANAGE_CONTENT_BITS = 0x0000_00b0;
        /// Permission holder can create, edit, and delete books.
        const EDIT_BOOK = 0x0000_0010;
        /// Permission holder can create, edit, and delete modules.
        const EDIT_MODULE = 0x0000_0020;
        /// All bits allocated for role management permissions.
        const MANAGE_ROLES_BITS = 0x0000_0f00;
        /// Create, edit, and delete roles.
        const EDIT_ROLE = 0x0000_0100;
        /// All bits allocated for editing process management permissions.
        const MANAGE_PROCESS_BITS = 0x000f_0000;
        /// Permission holder can create, edit, and delete editing processes.
        const EDIT_PROCESS = 0x0001_0000;
        /// Permission holder can begin and manage editing process for specific
        /// modules.
        const MANAGE_PROCESS = 0x0002_0000;
        /// All bits allocated for resource management permissions.
        const MANAGE_RESOURCES_BITS = 0x0010_0000;
        /// Manage resources.
        const MANAGE_RESOURCES = 0x0010_0000;
    }
}

impl PermissionBits for TeamPermissions {
    fn empty() -> Self { Self::empty() }

    fn from_bits(bits: i32) -> Option<Self> { Self::from_bits(bits) }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "member:add" => Some(Self::ADD_MEMBER),
            "member:remove" => Some(Self::REMOVE_MEMBER),
            "member:edit-permissions" => Some(Self::EDIT_MEMBER_PERMISSIONS),
            "member:assign-role" => Some(Self::ASSIGN_ROLE),
            "role:edit" => Some(Self::EDIT_ROLE),
            "book:edit" => Some(Self::EDIT_BOOK),
            "module:edit" => Some(Self::EDIT_MODULE),
            "editing-process:edit" => Some(Self::EDIT_PROCESS),
            "editing-process:manage" => Some(Self::MANAGE_PROCESS),
            "resources:manage" => Some(Self::MANAGE_RESOURCES),
            _ => None,
        }
    }

    fn require(self, permissions: TeamPermissions)
    -> Result<(), RequirePermissionsError<Self>> {
        if self.contains(permissions) {
            Ok(())
        } else {
            log::trace!("Missing permissions: {:?}", permissions - self);
            Err(RequirePermissionsError(permissions - self))
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ADD_MEMBER => "member:add",
            Self::REMOVE_MEMBER => "member:remove",
            Self::EDIT_MEMBER_PERMISSIONS => "member:edit-permissions",
            Self::ASSIGN_ROLE => "member:assign-role",
            Self::EDIT_ROLE => "role:edit",
            Self::EDIT_BOOK => "book:edit",
            Self::EDIT_MODULE => "module:edit",
            Self::EDIT_PROCESS => "editing-process:edit",
            Self::MANAGE_PROCESS => "editing-process:manage",
            Self::MANAGE_RESOURCES => "resources:manage",
            _ if Self::MANAGE_MEMBERS_BITS.contains(self) => "member",
            _ if Self::MANAGE_ROLES_BITS.contains(self) => "role",
            _ if Self::MANAGE_PROCESS_BITS.contains(self) => "editing-process",
            _ if Self::MANAGE_RESOURCES.contains(self) => "resources",
            _ => "*",
        }
    }

    fn insert(&mut self, permissions: Self) { self.insert(permissions); }
}

pub trait Permission {
    type Bits: PermissionBits;

    /// Permissions are stored as bit-flags, and this field is a mask of bits
    /// corresponding to this permission (or combination of permissions).
    fn bits() -> Self::Bits;
}

macro_rules! permission {
    (
        $name:ident: $ty:ty = $value:ident
    ) => {
        pub struct $name;

        impl Permission for $name {
            type Bits = $ty;

            #[inline]
            fn bits() -> Self::Bits {
                <$ty>::$value
            }
        }
    };
}

permission!(AddMember: TeamPermissions = ADD_MEMBER);
permission!(RemoveMember: TeamPermissions = REMOVE_MEMBER);
permission!(EditBook: TeamPermissions = EDIT_BOOK);
permission!(EditModule: TeamPermissions = EDIT_MODULE);
permission!(EditRole: TeamPermissions = EDIT_ROLE);
permission!(EditProcess: TeamPermissions = EDIT_PROCESS);
permission!(ManageProcess: TeamPermissions = MANAGE_PROCESS);
permission!(ManageResources: TeamPermissions = MANAGE_RESOURCES);

pub struct NoPermissions<P>(PhantomData<*const P>);

macro_rules! impl_permissons {
    {
        $( ($first:ident $(, $name:ident)*) );+ $(;)*
    } => {
        $(
            impl<$first $(, $name)+> Permission for ($first $(, $name)+)
            where
                $first: Permission,
                $first::Bits: std::ops::BitOr<Output = $first::Bits>,
                $($name: Permission<Bits = $first::Bits>,)+
            {
                type Bits = $first::Bits;

                #[inline]
                fn bits() -> Self::Bits {
                    $($name::bits())|+
                }
            }
        )+
    };
}

impl_permissons! {
    (A, B);
    (A, B, C);
}

impl<P: PermissionBits> Permission for NoPermissions<P> {
    type Bits = P;

    fn bits() -> P { P::empty() }
}

impl ser::Serialize for TeamPermissions {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        if !ser.is_human_readable() {
            return ser.serialize_i32(self.bits());
        }

        let mut seq = ser.serialize_seq(Some(self.bits().count_ones() as usize))?;
        if self.contains(TeamPermissions::ADD_MEMBER) {
            seq.serialize_element("member:add")?;
        }
        if self.contains(TeamPermissions::REMOVE_MEMBER) {
            seq.serialize_element("member:remove")?;
        }
        if self.contains(TeamPermissions::EDIT_MEMBER_PERMISSIONS) {
            seq.serialize_element("member:edit-permissions")?;
        }
        if self.contains(TeamPermissions::ASSIGN_ROLE) {
            seq.serialize_element("member:assign-role")?;
        }
        if self.contains(TeamPermissions::EDIT_BOOK) {
            seq.serialize_element("book:edit")?;
        }
        if self.contains(TeamPermissions::EDIT_MODULE) {
            seq.serialize_element("module:edit")?;
        }
        if self.contains(TeamPermissions::EDIT_ROLE) {
            seq.serialize_element("role:edit")?;
        }
        if self.contains(TeamPermissions::EDIT_PROCESS) {
            seq.serialize_element("editing-process:edit")?;
        }
        if self.contains(TeamPermissions::MANAGE_PROCESS) {
            seq.serialize_element("editing-process:manage")?;
        }
        if self.contains(TeamPermissions::MANAGE_RESOURCES) {
            seq.serialize_element("resources:manage")?;
        }
        seq.end()
    }
}

impl<'de> de::Deserialize<'de> for TeamPermissions {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        if !de.is_human_readable() {
            de.deserialize_i32(BitsVisitor(PhantomData))
        } else {
            de.deserialize_any(BitsVisitor(PhantomData))
        }
    }
}

struct BitsVisitor<B>(PhantomData<B>);

impl<'de, B> de::Visitor<'de> for BitsVisitor<B>
where
    B: PermissionBits + de::Deserialize<'de>,
{
    type Value = B;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "a set of permissions")
    }

    fn visit_i64<E>(self, v: i64) -> Result<B, E>
    where
        E: de::Error,
    {
        if v < std::i32::MIN.into() || v > std::i32::MAX.into() {
            return Err(E::invalid_type(
                de::Unexpected::Signed(v), &"a 32-bit integer"));
        }

        PermissionBits::from_bits(v as i32)
            .ok_or_else(|| E::invalid_value(
                de::Unexpected::Signed(v), &"a bit-flag of permissions"))
    }

    fn visit_u64<E>(self, v: u64) -> Result<B, E>
    where
        E: de::Error,
    {
        self.visit_i64(v as i64)
    }

    fn visit_str<E>(self, v: &str) -> Result<B, E>
    where
        E: de::Error,
    {
        PermissionBits::from_str(v)
            .ok_or_else(|| E::invalid_value(
                de::Unexpected::Str(v), &"a permission name"))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<B, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut bits = <B as PermissionBits>::empty();

        while let Some(permission) = seq.next_element()? {
            bits.insert(permission);
        }

        Ok(bits)
    }
}
