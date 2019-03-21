use serde::{de, ser::{self, SerializeSeq}};
use std::fmt;

bitflags! {
    /// Permissions allow for a fine-grained control over what actions a given
    /// user can take.
    pub struct PermissionBits: i32 {
        /// All bits allocated for user management permissions.
        const MANAGE_USERS_BITS = 0x0000000f;
        /// Permission holder can invite new users into the platform.
        const INVITE_USER = 0x00000001;
        /// Permission holder can remove existing users from the platform.
        const DELETE_USER = 0x00000002;
        /// Permission holder can change other user's permissions.
        const EDIT_USER_PERMISSIONS = 0x00000004;
        /// All bits allocated for content management permissions.
        const MANAGE_CONTENT_BITS = 0x000000f0;
        /// Permission holder can create, edit, and delete books.
        const EDIT_BOOK = 0x00000010;
        /// Permission holder can create, edit, and delete modules.
        const EDIT_MODULE = 0x00000020;
    }
}

impl PermissionBits {
    /// Get set of all elevated permissions.
    #[inline]
    pub fn elevated() -> PermissionBits {
        PermissionBits::all()
    }

    /// Get set of all (non-elevated) permissions.
    #[inline]
    pub fn normal() -> PermissionBits {
        PermissionBits::empty()
    }

    /// Verify that all required permissions are present.
    ///
    /// This is the same check as `self.contains(permissions)`, but returns an
    /// [`ApiError`].
    pub fn require(&self, permissions: PermissionBits)
    -> Result<(), RequirePermissionsError> {
        if self.contains(permissions) {
            Ok(())
        } else {
            Err(RequirePermissionsError(permissions - *self))
        }
    }
}

pub trait Permission {
    /// Permissions are stored as bit-flags, and this field is a mask of bits
    /// corresponding to this permission (or combination of permissions).
    fn bits() -> PermissionBits;
}

macro_rules! permission {
    (
        $name:ident = $value:expr
    ) => {
        pub struct $name;

        impl Permission for $name {
            #[inline]
            fn bits() -> PermissionBits {
                $value
            }
        }
    };
}

permission!(InviteUser = PermissionBits::INVITE_USER);
permission!(DeleteUser = PermissionBits::DELETE_USER);
permission!(EditUserPermissions = PermissionBits::EDIT_USER_PERMISSIONS);
permission!(EditBook = PermissionBits::EDIT_BOOK);
permission!(EditModule = PermissionBits::EDIT_MODULE);

#[derive(ApiError, Debug, Fail)]
#[api(status = "FORBIDDEN", code = "user:insufficient-permissions")]
#[fail(display = "Missing required permissions: {:?}", _0)]
pub struct RequirePermissionsError(PermissionBits);

macro_rules! impl_permissons {
    {
        $( ($($name:ident),+) );+ $(;)*
    } => {
        $(
            impl<$($name),+> Permission for ($($name),+)
            where
                $($name: Permission,)+
            {
                #[inline]
                fn bits() -> PermissionBits {
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

impl ser::Serialize for PermissionBits {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut seq = ser.serialize_seq(Some(self.bits().count_ones() as usize))?;
        if self.contains(PermissionBits::INVITE_USER) {
            seq.serialize_element("user:invite")?;
        }
        if self.contains(PermissionBits::DELETE_USER) {
            seq.serialize_element("user:delete")?;
        }
        if self.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
            seq.serialize_element("user:edit-permissions")?;
        }
        if self.contains(PermissionBits::EDIT_BOOK) {
            seq.serialize_element("book:edit")?;
        }
        if self.contains(PermissionBits::EDIT_MODULE) {
            seq.serialize_element("module:edit")?;
        }
        seq.end()
    }
}

impl<'de> de::Deserialize<'de> for PermissionBits {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        de.deserialize_any(BitsVisitor)
    }
}

struct BitsVisitor;

impl<'de> de::Visitor<'de> for BitsVisitor {
    type Value = PermissionBits;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "a set of permissions")
    }

    fn visit_i64<E>(self, v: i64) -> Result<PermissionBits, E>
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

    fn visit_u64<E>(self, v: u64) -> Result<PermissionBits, E>
    where
        E: de::Error,
    {
        self.visit_i64(v as i64)
    }

    fn visit_str<E>(self, v: &str) -> Result<PermissionBits, E>
    where
        E: de::Error,
    {
        Ok(match v {
            "user:invite" => PermissionBits::INVITE_USER,
            "user:delete" => PermissionBits::DELETE_USER,
            "user:edit-permissions" => PermissionBits::EDIT_USER_PERMISSIONS,
            "book:edit" => PermissionBits::EDIT_BOOK,
            "module:edit" => PermissionBits::EDIT_MODULE,
            _ => return Err(E::invalid_value(
                de::Unexpected::Str(v), &"a permission name")),
        })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<PermissionBits, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut bits = PermissionBits::empty();

        while let Some(permission) = seq.next_element()? {
            bits.insert(permission);
        }

        Ok(bits)
    }
}
