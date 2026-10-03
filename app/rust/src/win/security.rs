//! Token membership and privilege helpers.

use windows::Win32::Security::{
    AllocateAndInitializeSid, CheckTokenMembership, FreeSid, PSID, SECURITY_NT_AUTHORITY,
};
use windows::core::BOOL;

struct OwnedSid(PSID);
impl Drop for OwnedSid {
    fn drop(&mut self) {
        // SAFETY: This SID was allocated by AllocateAndInitializeSid and is owned here.
        unsafe { FreeSid(self.0) };
    }
}

/// Checks enabled membership in the built-in Administrators group for this token.
pub fn is_admin() -> bool {
    let mut sid = PSID::default();
    // SAFETY: The authority and output pointer are valid; two RID subauthorities are supplied.
    if unsafe {
        AllocateAndInitializeSid(
            &SECURITY_NT_AUTHORITY,
            2,
            32,
            544,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut sid,
        )
    }
    .is_err()
    {
        eprintln!("{}", super::Error::last("AllocateAndInitializeSid"));
        return false;
    }
    let sid = OwnedSid(sid);
    let mut member = BOOL::default();
    // SAFETY: sid is live; a null token selects the effective token and member is writable.
    match unsafe { CheckTokenMembership(None, sid.0, &mut member) } {
        Ok(()) => member.as_bool(),
        Err(error) => {
            eprintln!("{}", super::Error::from_win("CheckTokenMembership", error));
            false
        }
    }
}

/// Enables a named token privilege, returning false if it cannot be assigned.
pub fn enable_privilege(_name: &str) -> bool {
    eprintln!("enable_privilege not ported yet");
    false
}
