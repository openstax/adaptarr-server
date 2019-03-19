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
