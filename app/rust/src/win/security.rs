//! Token membership and privilege helpers.

use super::{Error, OwnedHandle, Result, wide};
use windows::Win32::Foundation::{ERROR_SUCCESS, GetLastError, HANDLE, LUID, SetLastError};
use windows::Win32::Security::{
    AdjustTokenPrivileges, AllocateAndInitializeSid, CheckTokenMembership, FreeSid,
    LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, PSID, SE_PRIVILEGE_ENABLED, SECURITY_NT_AUTHORITY,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{BOOL, PCWSTR};

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
pub fn enable_privilege(name: &str) -> bool {
    match try_enable_privilege(name) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

fn try_enable_privilege(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('\0') {
        return Err(Error::msg(
            "LookupPrivilegeValueW",
            "invalid privilege name",
        ));
    }
    let mut token = HANDLE::default();
    // C# parity: Services/Win32/EventLogApi.cs:386
    // SAFETY: The pseudo process handle is borrowed; token is a writable output.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    }
    .map_err(|e| Error::from_win("OpenProcessToken", e))?;
    // SAFETY: OpenProcessToken returned a uniquely owned, CloseHandle-compatible handle.
    let token = unsafe { OwnedHandle::from_raw(token) }?;
    let name = wide::to_wide(name);
    let mut luid = LUID::default();
    // SAFETY: The NUL-terminated name and writable LUID live through the call.
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), PCWSTR(name.as_ptr()), &mut luid) }
        .map_err(|e| Error::from_win("LookupPrivilegeValueW", e))?;
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: The token and SDK structure are live; no previous-state buffer is requested.
    unsafe {
        SetLastError(ERROR_SUCCESS);
        AdjustTokenPrivileges(token.as_raw(), false, Some(&privileges), 0, None, None)
    }
    .map_err(|e| Error::from_win("AdjustTokenPrivileges", e))?;
    // BOOL success can still mean ERROR_NOT_ALL_ASSIGNED; capture before closing the token.
    // SAFETY: GetLastError only reads the calling thread's error state.
    if unsafe { GetLastError() } != ERROR_SUCCESS {
        return Err(Error::last("AdjustTokenPrivileges"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::ERROR_NOT_ALL_ASSIGNED;

    #[test]
    fn assigned_and_missing_privileges_report_their_results() {
        assert!(enable_privilege("SeChangeNotifyPrivilege"));
        assert!(!enable_privilege("HWIDCheckerPhase1MissingPrivilege"));
        assert!(!enable_privilege("SeSecurityPrivilege\0ignored"));
    }

    #[test]
    fn unassigned_privilege_is_reported() {
        // Ordinary user and administrator tokens do not carry the trusted-computing-base privilege.
        let error =
            try_enable_privilege("SeTcbPrivilege").expect_err("this check must return an error");
        assert_eq!(error.code, ERROR_NOT_ALL_ASSIGNED.0);
        assert!(!enable_privilege("SeTcbPrivilege"));
    }
}
